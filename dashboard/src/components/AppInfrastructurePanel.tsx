import { type SubmitEvent, useCallback, useEffect, useMemo, useState } from 'react';
import type { CronEntry, NetworkRecord, StorageMount } from '../lib/api';
import {
  addCronEntry,
  addStorageMount,
  attachAppNetwork,
  detachAppNetwork,
  ensureStorageDirectory,
  fetchAppNetworks,
  fetchCronEntries,
  fetchNetworks,
  fetchStorageMounts,
  removeCronEntry,
  removeStorageMount,
} from '../lib/api';
import SelectField from './SelectField';
import TableScroll from './TableScroll';

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
  const [attachNetworkName, setAttachNetworkName] = useState('');
  const [hostPath, setHostPath] = useState('');
  const [containerPath, setContainerPath] = useState('');
  const [ensurePath, setEnsurePath] = useState('');
  const [cronSchedule, setCronSchedule] = useState('');
  const [cronCommand, setCronCommand] = useState('');
  const [fieldError, setFieldError] = useState<{ field: string; message: string } | null>(null);

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

  function flagFieldError(field: string, message: string) {
    setFieldError({ field, message });
    document.getElementById(field)?.focus();
  }

  async function handleAttachNetwork(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = attachNetworkName.trim();
    if (!name) {
      flagFieldError('attach-network', 'Select a network to attach.');
      return;
    }

    setFieldError(null);

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

  async function handleAddMount(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextHostPath = hostPath.trim();
    const nextContainerPath = containerPath.trim();
    if (!nextHostPath) {
      flagFieldError('host-path', 'Enter the host path to mount.');
      return;
    }

    if (!nextContainerPath) {
      flagFieldError('container-path', 'Enter the container path to mount it at.');
      return;
    }

    setFieldError(null);

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

  async function handleEnsureDirectory(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const path = ensurePath.trim();
    if (!path) {
      flagFieldError('ensure-path', 'Enter the directory path to create.');
      return;
    }

    setFieldError(null);

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

  async function handleAddCron(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const schedule = cronSchedule.trim();
    const command = cronCommand.trim();
    if (!schedule) {
      flagFieldError('cron-schedule', 'Enter a cron schedule, for example 0 * * * *.');
      return;
    }

    if (!command) {
      flagFieldError('cron-command', 'Enter the command to run on that schedule.');
      return;
    }

    setFieldError(null);

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
    return <InfrastructurePanelSkeleton />;
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
          <div className="panel-heading panel-heading-top">
            <div className="stack-sm panel-heading-copy">
              <p className="eyebrow">Networks</p>
              <h2 className="section-title">Attached networks</h2>
              <p className="page-copy">
                Shared Docker networks this app can reach. Create or remove networks from the
                overview.
              </p>
            </div>
            <span className="inventory-summary">{state.attachedNetworks.length} attached</span>
          </div>

          {state.attachedNetworks.length === 0 ? (
            <p className="text-muted">This app is not on any shared network yet.</p>
          ) : (
            <ul className="network-list">
              {state.attachedNetworks.map((network) => (
                <li key={network.id} className="network-row">
                  <span className="font-mono network-row-name">{network.name}</span>
                  <button
                    type="button"
                    className="btn btn-outline btn-danger-outline btn-sm"
                    disabled={locked || busy === `network-detach-${network.name}`}
                    onClick={() => handleDetachNetwork(network.name)}
                  >
                    {busy === `network-detach-${network.name}` ? 'Detaching…' : 'Detach'}
                  </button>
                </li>
              ))}
            </ul>
          )}

          {attachableNetworks.length > 0 ? (
            <form onSubmit={handleAttachNetwork} className="network-attach-form" noValidate>
              <div className="form-group">
                <label className="form-label" htmlFor="attach-network">
                  Attach a network
                </label>
                <SelectField
                  id="attach-network"
                  value={attachNetworkName}
                  onChange={(next) => {
                    setAttachNetworkName(next);
                    if (fieldError?.field === 'attach-network') setFieldError(null);
                  }}
                  invalid={fieldError?.field === 'attach-network'}
                  describedBy={
                    fieldError?.field === 'attach-network' ? 'attach-network-error' : undefined
                  }
                  disabled={locked || busy !== null}
                  placeholder="Select network"
                  emptyMessage="No other host networks are available"
                  options={attachableNetworks.map((network) => ({
                    value: network.name,
                    label: network.name,
                  }))}
                />
                <FieldError field="attach-network" error={fieldError} />
              </div>
              <button
                className="btn btn-secondary"
                type="submit"
                disabled={locked || busy !== null}
              >
                {busy?.startsWith('network-attach-') ? 'Attaching…' : 'Attach'}
              </button>
            </form>
          ) : (
            <p className="text-muted">
              {state.networks.length === 0
                ? 'No shared networks exist on this host yet.'
                : 'Every shared network on this host is already attached.'}
            </p>
          )}
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
                onChange={(event) => {
                  setHostPath(event.target.value);
                  if (fieldError?.field === 'host-path') setFieldError(null);
                }}
                placeholder="/var/lib/deku/apps/demo/data"
                aria-invalid={fieldError?.field === 'host-path' ? true : undefined}
                aria-describedby={fieldError?.field === 'host-path' ? 'host-path-error' : undefined}
                disabled={locked || busy !== null}
              />
              <FieldError field="host-path" error={fieldError} />
            </div>
            <div className="form-group">
              <label className="form-label" htmlFor="container-path">
                Container path
              </label>
              <input
                id="container-path"
                className="input"
                value={containerPath}
                onChange={(event) => {
                  setContainerPath(event.target.value);
                  if (fieldError?.field === 'container-path') setFieldError(null);
                }}
                placeholder="/app/data"
                aria-invalid={fieldError?.field === 'container-path' ? true : undefined}
                aria-describedby={
                  fieldError?.field === 'container-path' ? 'container-path-error' : undefined
                }
                disabled={locked || busy !== null}
              />
              <FieldError field="container-path" error={fieldError} />
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
                onChange={(event) => {
                  setEnsurePath(event.target.value);
                  if (fieldError?.field === 'ensure-path') setFieldError(null);
                }}
                placeholder="/var/lib/deku/apps/demo/data"
                aria-invalid={fieldError?.field === 'ensure-path' ? true : undefined}
                aria-describedby={
                  fieldError?.field === 'ensure-path' ? 'ensure-path-error' : undefined
                }
                disabled={locked || busy !== null}
              />
              <FieldError field="ensure-path" error={fieldError} />
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
            <TableScroll>
              <table className="table">
                <caption className="sr-only">Storage mounts for this app</caption>
                <thead>
                  <tr>
                    <th scope="col">Host path</th>
                    <th scope="col">Container path</th>
                    <th scope="col">
                      <span className="sr-only">Actions</span>
                    </th>
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
                          className="btn btn-outline btn-danger-outline btn-sm"
                          aria-label={`Remove mount ${mount.host_path} to ${mount.container_path}`}
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
            </TableScroll>
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
              onChange={(event) => {
                setCronSchedule(event.target.value);
                if (fieldError?.field === 'cron-schedule') setFieldError(null);
              }}
              placeholder="0 * * * *"
              aria-invalid={fieldError?.field === 'cron-schedule' ? true : undefined}
              aria-describedby={
                fieldError?.field === 'cron-schedule' ? 'cron-schedule-error' : undefined
              }
              disabled={locked || busy !== null}
            />
            <FieldError field="cron-schedule" error={fieldError} />
          </div>
          <div className="form-group">
            <label className="form-label" htmlFor="cron-command">
              Command
            </label>
            <input
              id="cron-command"
              className="input"
              value={cronCommand}
              onChange={(event) => {
                setCronCommand(event.target.value);
                if (fieldError?.field === 'cron-command') setFieldError(null);
              }}
              placeholder="bundle exec rake jobs:run"
              aria-invalid={fieldError?.field === 'cron-command' ? true : undefined}
              aria-describedby={
                fieldError?.field === 'cron-command' ? 'cron-command-error' : undefined
              }
              disabled={locked || busy !== null}
            />
            <FieldError field="cron-command" error={fieldError} />
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
          <TableScroll>
            <table className="table">
              <caption className="sr-only">Scheduled commands for this app</caption>
              <thead>
                <tr>
                  <th scope="col">Schedule</th>
                  <th scope="col">Command</th>
                  <th scope="col">
                    <span className="sr-only">Actions</span>
                  </th>
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
                        className="btn btn-outline btn-danger-outline btn-sm"
                        aria-label={`Remove cron entry ${entry.schedule}`}
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
          </TableScroll>
        )}
      </article>
    </section>
  );
}

type FieldErrorState = { field: string; message: string } | null;

function FieldError({ field, error }: { field: string; error: FieldErrorState }) {
  if (error?.field !== field) return null;

  return (
    <p id={`${field}-error`} className="form-error">
      {error.message}
    </p>
  );
}

function InfrastructurePanelSkeleton() {
  return (
    <section className="stack-lg">
      <div className="panel-grid">
        <article className="panel stack-md">
          <div className="panel-heading">
            <div className="stack-sm panel-heading-copy">
              <SkeletonBlock className="h-3 w-20" />
              <SkeletonBlock className="h-7 w-40" />
              <SkeletonBlock className="h-4 w-full max-w-md" />
              <SkeletonBlock className="h-4 w-4/5 max-w-sm" />
            </div>
            <SkeletonBlock className="h-5 w-24" />
          </div>

          <div className="stack-md">
            {skeletonItems('network-field', 1).map((item) => (
              <div key={item} className="form-group">
                <SkeletonBlock className="h-3 w-28" />
                <SkeletonBlock className="h-11 w-full" />
                <div className="form-actions">
                  <SkeletonBlock className="h-10 w-32" />
                </div>
              </div>
            ))}
          </div>

          <div className="stack-sm">
            {skeletonItems('network-row', 3).map((item) => (
              <SkeletonBlock key={item} className="h-12 w-full" />
            ))}
          </div>
        </article>

        <article className="panel stack-md">
          <div className="stack-sm">
            <SkeletonBlock className="h-3 w-20" />
            <SkeletonBlock className="h-7 w-48" />
            <SkeletonBlock className="h-4 w-full max-w-md" />
            <SkeletonBlock className="h-4 w-4/5 max-w-sm" />
          </div>

          <div className="stack-md">
            {skeletonItems('storage-field', 2).map((item) => (
              <div key={item} className="form-group">
                <SkeletonBlock className="h-3 w-28" />
                <SkeletonBlock className="h-11 w-full" />
              </div>
            ))}
            <div className="form-actions">
              <SkeletonBlock className="h-10 w-32" />
            </div>
          </div>

          <div className="stack-md">
            <div className="form-group">
              <SkeletonBlock className="h-3 w-32" />
              <SkeletonBlock className="h-11 w-full" />
            </div>
            <div className="form-actions">
              <SkeletonBlock className="h-10 w-36" />
            </div>
          </div>

          <div className="stack-sm">
            {skeletonItems('storage-row', 2).map((item) => (
              <SkeletonBlock key={item} className="h-12 w-full" />
            ))}
          </div>
        </article>
      </div>

      <article className="panel stack-md">
        <div className="panel-heading">
          <div className="stack-sm panel-heading-copy">
            <SkeletonBlock className="h-3 w-16" />
            <SkeletonBlock className="h-7 w-52" />
            <SkeletonBlock className="h-4 w-full max-w-md" />
          </div>
          <SkeletonBlock className="h-5 w-24" />
        </div>

        <div className="stack-md">
          {skeletonItems('cron-field', 2).map((item) => (
            <div key={item} className="form-group">
              <SkeletonBlock className="h-3 w-28" />
              <SkeletonBlock className="h-11 w-full" />
            </div>
          ))}
          <div className="form-actions">
            <SkeletonBlock className="h-10 w-28" />
          </div>
        </div>

        <div className="stack-sm">
          {skeletonItems('cron-row', 3).map((item) => (
            <SkeletonBlock key={item} className="h-12 w-full" />
          ))}
        </div>
      </article>
    </section>
  );
}

function SkeletonBlock({ className }: { className: string }) {
  return <div className={`app-skeleton-block animate-pulse rounded-md ${className}`} />;
}

function skeletonItems(prefix: string, count: number): string[] {
  return Array.from({ length: count }, (_, index) => `${prefix}-${index}`);
}
