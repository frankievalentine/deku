import { type FormEvent, useCallback, useEffect, useMemo, useState } from 'react';
import type { CronEntry, NetworkRecord, StorageMount } from '../lib/api';
import {
  addCronEntry,
  addStorageMount,
  attachAppNetwork,
  createNetwork,
  deleteNetwork,
  detachAppNetwork,
  ensureStorageDirectory,
  fetchAppNetworks,
  fetchCronEntries,
  fetchNetworks,
  fetchStorageMounts,
  removeCronEntry,
  removeStorageMount,
} from '../lib/api';

interface AppInfrastructurePanelProps {
  appName: string;
  locked: boolean;
}

interface InfrastructureState {
  networks: NetworkRecord[];
  attachedNetworks: NetworkRecord[];
  mounts: StorageMount[];
  cron: CronEntry[];
}

export default function AppInfrastructurePanel({ appName, locked }: AppInfrastructurePanelProps) {
  const [state, setState] = useState<InfrastructureState | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [networkName, setNetworkName] = useState('');
  const [attachNetworkName, setAttachNetworkName] = useState('');
  const [hostPath, setHostPath] = useState('');
  const [containerPath, setContainerPath] = useState('');
  const [ensurePath, setEnsurePath] = useState('');
  const [cronSchedule, setCronSchedule] = useState('');
  const [cronCommand, setCronCommand] = useState('');

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const [networks, attachedNetworks, mounts, cron] = await Promise.all([
        fetchNetworks(),
        fetchAppNetworks(appName),
        fetchStorageMounts(appName),
        fetchCronEntries(appName),
      ]);
      setState({ networks, attachedNetworks, mounts, cron });
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to load app infrastructure.'
      );
    } finally {
      setLoading(false);
    }
  }, [appName]);

  useEffect(() => {
    void load();
  }, [load]);

  const attachableNetworks = useMemo(() => {
    if (!state) return [];
    const attached = new Set(state.attachedNetworks.map((network) => network.name));
    return state.networks.filter((network) => !attached.has(network.name));
  }, [state]);

  async function handleCreateNetwork(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = networkName.trim();
    if (!name) return;

    try {
      setBusy('network-create');
      setError(null);
      setNotice(null);
      await createNetwork(name);
      setNetworkName('');
      await load();
      setNotice(`Created network ${name}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to create network.');
    } finally {
      setBusy(null);
    }
  }

  async function handleAttachNetwork(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = attachNetworkName.trim();
    if (!name) return;

    try {
      setBusy(`network-attach-${name}`);
      setError(null);
      setNotice(null);
      await attachAppNetwork(appName, name);
      setAttachNetworkName('');
      await load();
      setNotice(`Attached ${appName} to network ${name}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to attach network.');
    } finally {
      setBusy(null);
    }
  }

  async function handleDetachNetwork(name: string) {
    try {
      setBusy(`network-detach-${name}`);
      setError(null);
      setNotice(null);
      await detachAppNetwork(appName, name);
      await load();
      setNotice(`Detached ${appName} from network ${name}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to detach network.');
    } finally {
      setBusy(null);
    }
  }

  async function handleDeleteNetwork(name: string) {
    try {
      setBusy(`network-delete-${name}`);
      setError(null);
      setNotice(null);
      await deleteNetwork(name);
      await load();
      setNotice(`Deleted network ${name}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to delete network.');
    } finally {
      setBusy(null);
    }
  }

  async function handleAddMount(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextHostPath = hostPath.trim();
    const nextContainerPath = containerPath.trim();
    if (!nextHostPath || !nextContainerPath) return;

    try {
      setBusy('mount-add');
      setError(null);
      setNotice(null);
      await addStorageMount(appName, nextHostPath, nextContainerPath);
      setHostPath('');
      setContainerPath('');
      await load();
      setNotice(`Added mount ${nextHostPath} -> ${nextContainerPath}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to add storage mount.');
    } finally {
      setBusy(null);
    }
  }

  async function handleRemoveMount(mount: StorageMount) {
    try {
      setBusy(`mount-remove-${mount.id}`);
      setError(null);
      setNotice(null);
      await removeStorageMount(appName, mount.id);
      await load();
      setNotice(`Removed mount ${mount.host_path} -> ${mount.container_path}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to remove mount.');
    } finally {
      setBusy(null);
    }
  }

  async function handleEnsureDirectory(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const path = ensurePath.trim();
    if (!path) return;

    try {
      setBusy('ensure-dir');
      setError(null);
      setNotice(null);
      await ensureStorageDirectory(appName, path);
      setEnsurePath('');
      setNotice(`Ensured directory ${path}.`);
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to ensure storage directory.'
      );
    } finally {
      setBusy(null);
    }
  }

  async function handleAddCron(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const schedule = cronSchedule.trim();
    const command = cronCommand.trim();
    if (!schedule || !command) return;

    try {
      setBusy('cron-add');
      setError(null);
      setNotice(null);
      await addCronEntry(appName, schedule, command);
      setCronSchedule('');
      setCronCommand('');
      await load();
      setNotice(`Added cron entry ${schedule}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to add cron entry.');
    } finally {
      setBusy(null);
    }
  }

  async function handleRemoveCron(entry: CronEntry) {
    try {
      setBusy(`cron-remove-${entry.id}`);
      setError(null);
      setNotice(null);
      await removeCronEntry(appName, entry.id);
      await load();
      setNotice(`Removed cron entry ${entry.schedule}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to remove cron entry.');
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <span className="loading-spinner" />
        <span>Loading app infrastructure…</span>
      </div>
    );
  }

  if (!state) {
    return (
      <div className="panel error-state">
        Failed to load infrastructure: {error ?? 'unknown error'}
      </div>
    );
  }

  return (
    <section className="stack-lg">
      {notice ? <p className="callout callout-success">{notice}</p> : null}
      {error ? <p className="callout callout-danger">{error}</p> : null}

      <div className="panel-grid">
        <article className="panel stack-md">
          <div className="cluster justify-between align-start">
            <div className="stack-sm">
              <p className="eyebrow">Networks</p>
              <h2 className="section-title">Create and attach</h2>
              <p className="page-copy">
                Manage Docker networks for this app, including global network creation and app
                attachment.
              </p>
            </div>
            <span className="inventory-summary">{state.attachedNetworks.length} attached</span>
          </div>

          <form onSubmit={handleCreateNetwork} className="stack-md">
            <div className="form-group">
              <label className="form-label" htmlFor="network-name">
                New network
              </label>
              <input
                id="network-name"
                className="input"
                value={networkName}
                onChange={(event) => setNetworkName(event.target.value)}
                placeholder="private-backplane"
                disabled={locked || busy !== null}
              />
            </div>
            <div className="form-actions">
              <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
                {busy === 'network-create' ? 'Creating…' : 'Create network'}
              </button>
            </div>
          </form>

          <form onSubmit={handleAttachNetwork} className="stack-md">
            <div className="form-group">
              <label className="form-label" htmlFor="attach-network">
                Attach existing network
              </label>
              <select
                id="attach-network"
                className="input"
                value={attachNetworkName}
                onChange={(event) => setAttachNetworkName(event.target.value)}
                disabled={locked || busy !== null}
              >
                <option value="">Select network</option>
                {attachableNetworks.map((network) => (
                  <option key={network.id} value={network.name}>
                    {network.name}
                  </option>
                ))}
              </select>
            </div>
            <div className="form-actions">
              <button
                className="btn btn-secondary"
                type="submit"
                disabled={locked || busy !== null || attachNetworkName.length === 0}
              >
                {busy?.startsWith('network-attach-') ? 'Attaching…' : 'Attach network'}
              </button>
            </div>
          </form>

          <table className="table">
            <thead>
              <tr>
                <th>Name</th>
                <th>State</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {state.networks.map((network) => {
                const attached = state.attachedNetworks.some((item) => item.name === network.name);
                return (
                  <tr key={network.id}>
                    <td>{network.name}</td>
                    <td>{attached ? 'Attached' : 'Available'}</td>
                    <td>
                      <div className="button-row">
                        {attached ? (
                          <button
                            type="button"
                            className="btn btn-secondary btn-sm"
                            disabled={locked || busy === `network-detach-${network.name}`}
                            onClick={() => handleDetachNetwork(network.name)}
                          >
                            Detach
                          </button>
                        ) : null}
                        <button
                          type="button"
                          className="btn btn-danger btn-sm"
                          disabled={locked || busy === `network-delete-${network.name}`}
                          onClick={() => handleDeleteNetwork(network.name)}
                        >
                          Delete
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </article>

        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Storage</p>
            <h2 className="section-title">Mounts and directories</h2>
            <p className="page-copy">
              Manage persistent mount mappings and ensure host directories exist before wiring them
              into the app.
            </p>
          </div>

          <form onSubmit={handleAddMount} className="stack-md">
            <div className="form-group">
              <label className="form-label" htmlFor="host-path">
                Host path
              </label>
              <input
                id="host-path"
                className="input"
                value={hostPath}
                onChange={(event) => setHostPath(event.target.value)}
                placeholder="/var/lib/deku/apps/demo/data"
                disabled={locked || busy !== null}
              />
            </div>
            <div className="form-group">
              <label className="form-label" htmlFor="container-path">
                Container path
              </label>
              <input
                id="container-path"
                className="input"
                value={containerPath}
                onChange={(event) => setContainerPath(event.target.value)}
                placeholder="/app/data"
                disabled={locked || busy !== null}
              />
            </div>
            <div className="form-actions">
              <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
                {busy === 'mount-add' ? 'Adding…' : 'Add mount'}
              </button>
            </div>
          </form>

          <form onSubmit={handleEnsureDirectory} className="stack-md">
            <div className="form-group">
              <label className="form-label" htmlFor="ensure-path">
                Ensure directory
              </label>
              <input
                id="ensure-path"
                className="input"
                value={ensurePath}
                onChange={(event) => setEnsurePath(event.target.value)}
                placeholder="/var/lib/deku/apps/demo/data"
                disabled={locked || busy !== null}
              />
            </div>
            <div className="form-actions">
              <button
                className="btn btn-secondary"
                type="submit"
                disabled={locked || busy !== null}
              >
                {busy === 'ensure-dir' ? 'Ensuring…' : 'Ensure directory'}
              </button>
            </div>
          </form>

          {state.mounts.length === 0 ? (
            <p className="text-muted">No storage mounts have been configured for this app.</p>
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <th>Host path</th>
                  <th>Container path</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {state.mounts.map((mount) => (
                  <tr key={mount.id}>
                    <td className="font-mono">{mount.host_path}</td>
                    <td className="font-mono">{mount.container_path}</td>
                    <td>
                      <button
                        type="button"
                        className="btn btn-danger btn-sm"
                        disabled={locked || busy === `mount-remove-${mount.id}`}
                        onClick={() => handleRemoveMount(mount)}
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

      <article className="panel stack-md">
        <div className="cluster justify-between align-start">
          <div className="stack-sm">
            <p className="eyebrow">Cron</p>
            <h2 className="section-title">Scheduled commands</h2>
            <p className="page-copy">
              Define scheduled commands that run against this app’s runtime on the host.
            </p>
          </div>
          <span className="inventory-summary">{state.cron.length} entries</span>
        </div>

        <form onSubmit={handleAddCron} className="stack-md">
          <div className="form-group">
            <label className="form-label" htmlFor="cron-schedule">
              Schedule
            </label>
            <input
              id="cron-schedule"
              className="input"
              value={cronSchedule}
              onChange={(event) => setCronSchedule(event.target.value)}
              placeholder="0 * * * *"
              disabled={locked || busy !== null}
            />
          </div>
          <div className="form-group">
            <label className="form-label" htmlFor="cron-command">
              Command
            </label>
            <input
              id="cron-command"
              className="input"
              value={cronCommand}
              onChange={(event) => setCronCommand(event.target.value)}
              placeholder="bundle exec rake jobs:run"
              disabled={locked || busy !== null}
            />
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
              {busy === 'cron-add' ? 'Adding…' : 'Add cron'}
            </button>
          </div>
        </form>

        {state.cron.length === 0 ? (
          <p className="text-muted">No cron entries have been added for this app.</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Schedule</th>
                <th>Command</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {state.cron.map((entry) => (
                <tr key={entry.id}>
                  <td className="font-mono">{entry.schedule}</td>
                  <td className="font-mono">{entry.command}</td>
                  <td>
                    <button
                      type="button"
                      className="btn btn-danger btn-sm"
                      disabled={locked || busy === `cron-remove-${entry.id}`}
                      onClick={() => handleRemoveCron(entry)}
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
    </section>
  );
}
