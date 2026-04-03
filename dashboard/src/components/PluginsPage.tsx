import { type FormEvent, useEffect, useState } from 'react';
import { type Plugin, deletePlugin, fetchPlugins, getToken, installPlugin } from '../lib/api';
import ConnectScreen from './ConnectScreen';

export default function PluginsPage() {
  const [hasToken, setHasToken] = useState(() => Boolean(getToken()));

  if (!hasToken) {
    return <ConnectScreen onConnected={() => setHasToken(true)} />;
  }

  return <PluginsInner />;
}

function PluginsInner() {
  const [plugins, setPlugins] = useState<Plugin[]>([]);
  const [path, setPath] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void load();
  }, []);

  async function load() {
    try {
      setLoading(true);
      setError(null);
      setPlugins(await fetchPlugins());
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to load plugins.');
    } finally {
      setLoading(false);
    }
  }

  async function handleInstall(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextPath = path.trim();
    if (!nextPath) return;

    try {
      setBusy('install');
      await installPlugin(nextPath);
      setPath('');
      await load();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to install plugin.');
    } finally {
      setBusy(null);
    }
  }

  async function handleDelete(name: string) {
    try {
      setBusy(name);
      await deletePlugin(name);
      await load();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to uninstall plugin.');
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <span className="loading-spinner" />
        <span>Loading plugins…</span>
      </div>
    );
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <p className="eyebrow">Plugin registry</p>
          <h1 className="page-title">Installed plugins</h1>
          <p className="page-copy">
            Manage first-party and custom cdylib plugins loaded by the daemon.
          </p>
        </div>
      </section>

      <article className="panel stack-md">
        <div className="stack-sm">
          <p className="eyebrow">Install</p>
          <h2 className="section-title">Load plugin by path</h2>
        </div>
        <form onSubmit={handleInstall} className="stack-md">
          <div className="form-group">
            <label className="form-label" htmlFor="plugin-path">
              Shared library path
            </label>
            <input
              id="plugin-path"
              className="input"
              value={path}
              onChange={(event) => setPath(event.target.value)}
              placeholder="/home/deku/.deku/plugins/libdeku_plugin_postgres.so"
            />
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" type="submit" disabled={busy === 'install'}>
              {busy === 'install' ? 'Installing…' : 'Install plugin'}
            </button>
          </div>
        </form>
        {error && <p className="text-danger">{error}</p>}
      </article>

      <article className="panel stack-md">
        <div className="stack-sm">
          <p className="eyebrow">Inventory</p>
          <h2 className="section-title">Loaded now</h2>
        </div>
        {plugins.length === 0 ? (
          <p className="text-muted">No plugins are currently loaded.</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Version</th>
                <th>Path</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {plugins.map((plugin) => (
                <tr key={plugin.name}>
                  <td>{plugin.name}</td>
                  <td className="font-mono">{plugin.version ?? 'unknown'}</td>
                  <td className="font-mono">{plugin.path}</td>
                  <td>
                    <button
                      type="button"
                      className="btn btn-danger btn-sm"
                      disabled={busy === plugin.name}
                      onClick={() => handleDelete(plugin.name)}
                    >
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </article>
    </div>
  );
}
