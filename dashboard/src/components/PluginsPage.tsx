import { type SubmitEvent, useCallback, useEffect, useRef, useState } from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import { deletePlugin, fetchPlugins, installPlugin, type Plugin } from '../lib/api';
import ConfirmModal from './ConfirmModal';
import ConnectScreen from './ConnectScreen';
import TableScroll from './TableScroll';
import Spinner from './Spinner';

export default function PluginsPage() {
  const tokenAccess = useTokenAccess();

  if (tokenAccess === 'unknown') {
    return null;
  }

  if (tokenAccess === 'locked') {
    return <ConnectScreen />;
  }

  return <PluginsInner />;
}

function PluginsInner() {
  const [plugins, setPlugins] = useState<Plugin[]>([]);
  const [path, setPath] = useState('');
  const [pathError, setPathError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [removeTarget, setRemoveTarget] = useState<Plugin | null>(null);
  const pathInput = useRef<HTMLInputElement>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setLoadError(null);
      setPlugins(await fetchPlugins());
    } catch (nextError) {
      setLoadError(nextError instanceof Error ? nextError.message : 'Unable to load plugins.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleInstall(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextPath = path.trim();
    if (!nextPath) {
      setPathError('Enter the full path to the plugin shared library.');
      pathInput.current?.focus();
      return;
    }

    try {
      setBusy('install');
      setActionError(null);
      await installPlugin(nextPath);
      setPath('');
      await load();
    } catch (nextError) {
      setActionError(nextError instanceof Error ? nextError.message : 'Unable to install plugin.');
    } finally {
      setBusy(null);
    }
  }

  async function handleDelete() {
    if (!removeTarget) return;

    try {
      setBusy(removeTarget.name);
      setActionError(null);
      await deletePlugin(removeTarget.name);
      setRemoveTarget(null);
      await load();
    } catch (nextError) {
      setActionError(
        nextError instanceof Error ? nextError.message : 'Unable to uninstall plugin.'
      );
      setRemoveTarget(null);
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <Spinner />
        <span>Loading plugins…</span>
      </div>
    );
  }

  if (loadError) {
    return (
      <div className="panel error-state">
        <p className="text-danger" role="alert">
          {loadError}
        </p>
        <button type="button" className="btn btn-secondary" onClick={() => void load()}>
          Retry loading plugins
        </button>
      </div>
    );
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <h1 className="page-title">Plugins</h1>
          <p className="page-copy">
            Install a plugin from a shared library path to add the capabilities it provides.
          </p>
        </div>
      </section>

      <article className="panel stack-md">
        <h2 className="section-title">Install plugin</h2>
        <form onSubmit={handleInstall} className="stack-md" noValidate>
          <div className="form-group">
            <label className="form-label" htmlFor="plugin-path">
              Shared library path
            </label>
            <input
              id="plugin-path"
              ref={pathInput}
              className="input"
              value={path}
              onChange={(event) => {
                setPath(event.target.value);
                if (pathError) setPathError(null);
              }}
              placeholder="/home/deku/.deku/plugins/libdeku_plugin_postgres.so"
              spellCheck={false}
              autoComplete="off"
              required
              aria-invalid={pathError ? true : undefined}
              aria-describedby={
                pathError ? 'plugin-path-hint plugin-path-error' : 'plugin-path-hint'
              }
            />
            <p id="plugin-path-hint" className="text-muted">
              Use an absolute path to a plugin library the daemon can read.
            </p>
            {pathError ? (
              <p id="plugin-path-error" className="text-danger">
                {pathError}
              </p>
            ) : null}
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" type="submit" disabled={busy === 'install'}>
              {busy === 'install' ? <span className="loading-spinner" /> : null}
              <span>Install plugin</span>
            </button>
          </div>
        </form>
        {actionError ? (
          <p className="callout callout-danger" role="alert">
            {actionError}
          </p>
        ) : null}
      </article>

      <article className="panel stack-md">
        <h2 className="section-title">Loaded plugins</h2>
        {plugins.length === 0 ? (
          <p className="text-muted">
            No plugins are loaded. Install one by shared library path above to have the daemon load
            it.
          </p>
        ) : (
          <TableScroll>
            <table className="table">
              <caption className="sr-only">Plugins loaded by the daemon</caption>
              <thead>
                <tr>
                  <th scope="col">Name</th>
                  <th scope="col">Version</th>
                  <th scope="col">Path</th>
                  <th scope="col">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {plugins.map((plugin) => (
                  <tr key={plugin.name}>
                    <td>{plugin.name}</td>
                    <td className="font-mono">{plugin.version ?? 'Unknown'}</td>
                    <td className="font-mono">{plugin.path}</td>
                    <td>
                      <button
                        type="button"
                        className="btn btn-danger btn-sm"
                        aria-label={`Remove ${plugin.name}`}
                        disabled={busy !== null}
                        onClick={() => setRemoveTarget(plugin)}
                      >
                        Remove
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableScroll>
        )}
      </article>

      <ConfirmModal
        open={removeTarget !== null}
        title={`Remove ${removeTarget?.name ?? 'plugin'}?`}
        description="The daemon unloads this plugin right away, so the capabilities it provides stop working until the plugin is installed again."
        confirmLabel="Remove plugin"
        cancelLabel="Keep plugin"
        busy={removeTarget !== null && busy === removeTarget.name}
        onClose={() => {
          if (busy === null) setRemoveTarget(null);
        }}
        onConfirm={() => {
          void handleDelete();
        }}
      />
    </div>
  );
}
