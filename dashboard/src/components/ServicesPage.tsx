import { type FormEvent, useCallback, useEffect, useMemo, useState } from 'react';
import type {
  App,
  ManagedServiceDetail,
  ManagedServiceKind,
  ManagedServiceSummary,
  ServiceBackup,
} from '../lib/api';
import {
  createManagedService,
  deleteManagedService,
  fetchApps,
  fetchManagedService,
  fetchManagedServiceLogs,
  fetchManagedServices,
  fetchPostgresBackups,
  getToken,
  linkManagedService,
  restorePostgresBackup,
  triggerPostgresBackup,
  unlinkManagedService,
} from '../lib/api';
import ConfirmModal from './ConfirmModal';
import ConnectScreen from './ConnectScreen';

const SERVICE_KINDS: ManagedServiceKind[] = ['postgres', 'redis', 'mysql'];

const KIND_LABELS: Record<ManagedServiceKind, string> = {
  postgres: 'Postgres',
  redis: 'Redis',
  mysql: 'MySQL',
};

const KIND_DESCRIPTIONS: Record<ManagedServiceKind, string> = {
  postgres: 'Managed relational database instances with backup and restore controls.',
  redis: 'Managed Redis caches for ephemeral state, queues, and session workloads.',
  mysql: 'Managed MySQL instances for apps that need MySQL-compatible relational storage.',
};

interface ServiceTarget {
  kind: ManagedServiceKind;
  name: string;
}

interface RestoreTarget {
  serviceName: string;
  backup: ServiceBackup;
}

type ServiceMap<T> = Record<string, T>;
type KindRecord<T> = Record<ManagedServiceKind, T>;

export default function ServicesPage() {
  const [hasToken, setHasToken] = useState(() => Boolean(getToken()));

  if (!hasToken) {
    return <ConnectScreen onConnected={() => setHasToken(true)} />;
  }

  return <ServicesInner />;
}

function ServicesInner() {
  const [apps, setApps] = useState<App[]>([]);
  const [servicesByKind, setServicesByKind] = useState<KindRecord<ManagedServiceSummary[]>>(
    emptyKindRecord([])
  );
  const [selectedNames, setSelectedNames] = useState<KindRecord<string>>(emptyKindRecord(''));
  const [createDrafts, setCreateDrafts] = useState<KindRecord<string>>(emptyKindRecord(''));
  const [linkDrafts, setLinkDrafts] = useState<KindRecord<string>>(emptyKindRecord(''));
  const [activeKind, setActiveKind] = useState<ManagedServiceKind>('postgres');
  const [detailsByService, setDetailsByService] = useState<ServiceMap<ManagedServiceDetail>>({});
  const [backupsByService, setBackupsByService] = useState<ServiceMap<ServiceBackup[]>>({});
  const [logsByService, setLogsByService] = useState<ServiceMap<string[]>>({});
  const [loading, setLoading] = useState(true);
  const [detailLoadingKey, setDetailLoadingKey] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ServiceTarget | null>(null);
  const [restoreTarget, setRestoreTarget] = useState<RestoreTarget | null>(null);

  const loadOverview = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);

      const [nextApps, postgres, redis, mysql] = await Promise.all([
        fetchApps(),
        fetchManagedServices('postgres'),
        fetchManagedServices('redis'),
        fetchManagedServices('mysql'),
      ]);

      const nextServices = {
        postgres,
        redis,
        mysql,
      };

      setApps(nextApps);
      setServicesByKind(nextServices);
      setSelectedNames((current) => chooseSelectedNames(current, nextServices));
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to load managed services.');
    } finally {
      setLoading(false);
    }
  }, []);

  const loadServiceDetail = useCallback(async (kind: ManagedServiceKind, name: string) => {
    if (!name) return;

    const key = serviceKey(kind, name);

    try {
      setDetailLoadingKey(key);
      setError(null);

      const [detail, backups] = await Promise.all([
        fetchManagedService(kind, name),
        kind === 'postgres' ? fetchPostgresBackups(name) : Promise.resolve(null),
      ]);

      setDetailsByService((current) => ({ ...current, [key]: detail }));
      if (backups) {
        setBackupsByService((current) => ({ ...current, [key]: backups }));
      }
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : `Unable to load ${name} service detail.`
      );
    } finally {
      setDetailLoadingKey((current) => (current === key ? null : current));
    }
  }, []);

  useEffect(() => {
    void loadOverview();
  }, [loadOverview]);

  const activeServices = servicesByKind[activeKind];
  const activeServiceName = selectedNames[activeKind];
  const activeServiceKey = activeServiceName ? serviceKey(activeKind, activeServiceName) : null;
  const activeDetail = activeServiceKey ? (detailsByService[activeServiceKey] ?? null) : null;
  const activeBackups =
    activeKind === 'postgres' && activeServiceKey ? (backupsByService[activeServiceKey] ?? []) : [];
  const activeLogs = activeServiceKey ? (logsByService[activeServiceKey] ?? []) : [];

  useEffect(() => {
    if (!activeServiceName) return;
    void loadServiceDetail(activeKind, activeServiceName);
  }, [activeKind, activeServiceName, loadServiceDetail]);

  const totalServices = useMemo(
    () => SERVICE_KINDS.reduce((count, kind) => count + servicesByKind[kind].length, 0),
    [servicesByKind]
  );

  const linkedAppNames = useMemo(
    () => new Set((activeDetail?.links ?? []).map((link) => link.name)),
    [activeDetail]
  );

  const availableApps = useMemo(
    () => apps.filter((app) => !linkedAppNames.has(app.name)),
    [apps, linkedAppNames]
  );

  async function handleCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = createDrafts[activeKind].trim();
    if (!name) return;

    try {
      setBusy(`create-${activeKind}`);
      setError(null);
      setNotice(null);
      const created = await createManagedService(activeKind, name);
      setCreateDrafts((current) => ({ ...current, [activeKind]: '' }));
      setSelectedNames((current) => ({ ...current, [activeKind]: created.name }));
      await loadOverview();
      await loadServiceDetail(activeKind, created.name);
      setNotice(`${KIND_LABELS[activeKind]} service ${created.name} created.`);
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to create managed service.'
      );
    } finally {
      setBusy(null);
    }
  }

  async function handleDelete() {
    if (!deleteTarget) return;

    try {
      setBusy(`delete-${deleteTarget.kind}-${deleteTarget.name}`);
      setError(null);
      setNotice(null);
      await deleteManagedService(deleteTarget.kind, deleteTarget.name);
      setDeleteTarget(null);
      setDetailsByService((current) => {
        const next = { ...current };
        delete next[serviceKey(deleteTarget.kind, deleteTarget.name)];
        return next;
      });
      setBackupsByService((current) => {
        const next = { ...current };
        delete next[serviceKey(deleteTarget.kind, deleteTarget.name)];
        return next;
      });
      setLogsByService((current) => {
        const next = { ...current };
        delete next[serviceKey(deleteTarget.kind, deleteTarget.name)];
        return next;
      });
      await loadOverview();
      setNotice(`${KIND_LABELS[deleteTarget.kind]} service ${deleteTarget.name} removed.`);
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to delete managed service.'
      );
    } finally {
      setBusy(null);
    }
  }

  async function handleLink(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!activeServiceName) return;

    const appName = linkDrafts[activeKind].trim();
    if (!appName) return;

    try {
      setBusy(`link-${activeKind}-${activeServiceName}`);
      setError(null);
      setNotice(null);
      await linkManagedService(activeKind, activeServiceName, appName);
      setLinkDrafts((current) => ({ ...current, [activeKind]: '' }));
      await loadServiceDetail(activeKind, activeServiceName);
      setNotice(`${activeServiceName} linked to ${appName}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to link app.');
    } finally {
      setBusy(null);
    }
  }

  async function handleUnlink(appName: string) {
    if (!activeServiceName) return;

    try {
      setBusy(`unlink-${activeKind}-${activeServiceName}-${appName}`);
      setError(null);
      setNotice(null);
      await unlinkManagedService(activeKind, activeServiceName, appName);
      await loadServiceDetail(activeKind, activeServiceName);
      setNotice(`${appName} unlinked from ${activeServiceName}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to unlink app.');
    } finally {
      setBusy(null);
    }
  }

  async function handleLoadLogs() {
    if (!activeServiceName) return;

    try {
      setBusy(`logs-${activeKind}-${activeServiceName}`);
      setError(null);
      const logs = await fetchManagedServiceLogs(activeKind, activeServiceName, 120);
      setLogsByService((current) => ({
        ...current,
        [serviceKey(activeKind, activeServiceName)]: logs,
      }));
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to load service logs.');
    } finally {
      setBusy(null);
    }
  }

  async function handleBackup() {
    if (!activeServiceName || activeKind !== 'postgres') return;

    try {
      setBusy(`backup-${activeServiceName}`);
      setError(null);
      setNotice(null);
      await triggerPostgresBackup(activeServiceName);
      await loadServiceDetail('postgres', activeServiceName);
      setNotice(`Backup started for ${activeServiceName}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to create backup.');
    } finally {
      setBusy(null);
    }
  }

  async function handleRestore() {
    if (!restoreTarget) return;

    try {
      setBusy(`restore-${restoreTarget.backup.id}`);
      setError(null);
      setNotice(null);
      await restorePostgresBackup(restoreTarget.serviceName, restoreTarget.backup.id);
      setRestoreTarget(null);
      await loadServiceDetail('postgres', restoreTarget.serviceName);
      setNotice(
        `Restore started for ${restoreTarget.serviceName} using backup ${restoreTarget.backup.id.slice(0, 8)}.`
      );
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to restore backup.');
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <span className="loading-spinner" />
        <span>Loading managed services…</span>
      </div>
    );
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <p className="eyebrow">Managed services</p>
          <h1 className="page-title">Datastores and caches</h1>
          <p className="page-copy">
            Provision Postgres, Redis, and MySQL services, inspect connection details, link them to
            apps, and run service-level operations without leaving the dashboard.
          </p>
        </div>
        <div className="metrics-grid">
          <Metric label="Postgres" value={String(servicesByKind.postgres.length)} />
          <Metric label="Redis" value={String(servicesByKind.redis.length)} />
          <Metric label="MySQL" value={String(servicesByKind.mysql.length)} />
          <Metric label="Total" value={String(totalServices)} />
        </div>
      </section>

      <section className="panel stack-md">
        <div className="cluster justify-between align-center">
          <div className="stack-sm">
            <p className="eyebrow">Engines</p>
            <h2 className="section-title">Select a service type</h2>
          </div>
          <span className="inventory-summary">{KIND_LABELS[activeKind].toUpperCase()}</span>
        </div>

        <div className="segment-control" role="tablist" aria-label="Service engines">
          {SERVICE_KINDS.map((kind) => (
            <button
              key={kind}
              type="button"
              role="tab"
              aria-selected={activeKind === kind}
              className={`segment-button${activeKind === kind ? ' is-selected' : ''}`}
              onClick={() => setActiveKind(kind)}
            >
              <span>{KIND_LABELS[kind]}</span>
              <strong>{servicesByKind[kind].length}</strong>
            </button>
          ))}
        </div>

        <p className="page-copy">{KIND_DESCRIPTIONS[activeKind]}</p>

        {notice ? <p className="callout callout-success">{notice}</p> : null}
        {error ? <p className="callout callout-danger">{error}</p> : null}
      </section>

      <section className="panel-grid">
        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Provision</p>
            <h2 className="section-title">Create {KIND_LABELS[activeKind]}</h2>
          </div>

          <form onSubmit={handleCreate} className="stack-md">
            <div className="form-group">
              <label className="form-label" htmlFor="service-name">
                Service name
              </label>
              <input
                id="service-name"
                className="input"
                value={createDrafts[activeKind]}
                onChange={(event) =>
                  setCreateDrafts((current) => ({ ...current, [activeKind]: event.target.value }))
                }
                placeholder={`${activeKind}-main`}
              />
            </div>
            <div className="form-actions">
              <button
                className="btn btn-primary"
                type="submit"
                disabled={busy === `create-${activeKind}`}
              >
                {busy === `create-${activeKind}`
                  ? 'Creating…'
                  : `Create ${KIND_LABELS[activeKind]}`}
              </button>
            </div>
          </form>

          <div className="stack-sm">
            <p className="eyebrow">Inventory</p>
            <h3 className="deploy-title">Available now</h3>
          </div>

          {activeServices.length === 0 ? (
            <p className="text-muted">No {KIND_LABELS[activeKind]} services exist yet.</p>
          ) : (
            <div className="service-list">
              {activeServices.map((service) => {
                const selected = service.name === activeServiceName;
                return (
                  <button
                    key={service.id}
                    type="button"
                    className={`service-list-item${selected ? ' is-active' : ''}`}
                    onClick={() =>
                      setSelectedNames((current) => ({ ...current, [activeKind]: service.name }))
                    }
                  >
                    <div className="cluster justify-between align-center">
                      <strong>{service.name}</strong>
                      <ServiceState status={service.status} />
                    </div>
                    <span className="service-list-meta">
                      {service.container_id ? truncateId(service.container_id) : 'pending'}
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </article>

        <article className="panel stack-md">
          {!activeServiceName ? (
            <>
              <div className="stack-sm">
                <p className="eyebrow">Inspect</p>
                <h2 className="section-title">No service selected</h2>
              </div>
              <p className="page-copy">
                Create or select a {KIND_LABELS[activeKind]} service to inspect its connection
                details, links, logs, and service-specific operations.
              </p>
            </>
          ) : detailLoadingKey === activeServiceKey && !activeDetail ? (
            <div className="loading-state service-detail-loading">
              <span className="loading-spinner" />
              <span>Loading {activeServiceName}…</span>
            </div>
          ) : !activeDetail ? (
            <>
              <div className="stack-sm">
                <p className="eyebrow">Inspect</p>
                <h2 className="section-title">Service unavailable</h2>
              </div>
              <p className="text-danger">Unable to load detail for {activeServiceName}.</p>
            </>
          ) : (
            <>
              <div className="cluster justify-between align-start">
                <div className="stack-sm">
                  <p className="eyebrow">Service detail</p>
                  <h2 className="section-title">{activeDetail.name}</h2>
                  <p className="page-copy">
                    {KIND_LABELS[activeKind]} service created {formatDate(activeDetail.created_at)}.
                  </p>
                </div>
                <ServiceState status={activeDetail.status} />
              </div>

              <dl className="data-grid">
                <div>
                  <dt>Plugin</dt>
                  <dd className="font-mono">{activeDetail.plugin}</dd>
                </div>
                <div>
                  <dt>Container</dt>
                  <dd className="font-mono">
                    {activeDetail.container_id ? truncateId(activeDetail.container_id) : 'pending'}
                  </dd>
                </div>
                <div>
                  <dt>Links</dt>
                  <dd className="font-mono">{String(activeDetail.links.length)}</dd>
                </div>
                <div>
                  <dt>Engine</dt>
                  <dd>{KIND_LABELS[activeKind]}</dd>
                </div>
              </dl>

              <div className="stack-sm">
                <p className="eyebrow">Connection</p>
                <h3 className="deploy-title">Resolved connection values</h3>
              </div>

              <dl className="connection-grid">
                {Object.entries(activeDetail.connection).map(([key, value]) => (
                  <div key={key} className="connection-card">
                    <dt>{key}</dt>
                    <dd className="font-mono">{formatConnectionValue(value)}</dd>
                  </div>
                ))}
              </dl>

              <div className="stack-md">
                <div className="stack-sm">
                  <p className="eyebrow">Links</p>
                  <h3 className="deploy-title">Attached apps</h3>
                </div>

                <form onSubmit={handleLink} className="stack-md">
                  <div className="form-group">
                    <label className="form-label" htmlFor="link-app">
                      Link to app
                    </label>
                    <select
                      id="link-app"
                      className="input"
                      value={linkDrafts[activeKind]}
                      onChange={(event) =>
                        setLinkDrafts((current) => ({
                          ...current,
                          [activeKind]: event.target.value,
                        }))
                      }
                    >
                      <option value="">Select app</option>
                      {availableApps.map((app) => (
                        <option key={app.id} value={app.name}>
                          {app.name}
                        </option>
                      ))}
                    </select>
                  </div>
                  <div className="form-actions">
                    <button
                      className="btn btn-secondary"
                      type="submit"
                      disabled={
                        busy === `link-${activeKind}-${activeServiceName}` ||
                        linkDrafts[activeKind].length === 0
                      }
                    >
                      {busy === `link-${activeKind}-${activeServiceName}` ? 'Linking…' : 'Link app'}
                    </button>
                  </div>
                </form>

                {activeDetail.links.length === 0 ? (
                  <p className="text-muted">This service is not linked to any apps yet.</p>
                ) : (
                  <table className="table">
                    <thead>
                      <tr>
                        <th>App</th>
                        <th>Env key</th>
                        <th></th>
                      </tr>
                    </thead>
                    <tbody>
                      {activeDetail.links.map((link) => (
                        <tr key={`${link.name}-${link.env_key}`}>
                          <td>{link.name}</td>
                          <td className="font-mono">{link.env_key}</td>
                          <td>
                            <button
                              type="button"
                              className="btn btn-danger btn-sm"
                              disabled={
                                busy === `unlink-${activeKind}-${activeServiceName}-${link.name}`
                              }
                              onClick={() => handleUnlink(link.name)}
                            >
                              Unlink
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>

              <div className="stack-md">
                <div className="cluster justify-between align-center">
                  <div className="stack-sm">
                    <p className="eyebrow">Logs</p>
                    <h3 className="deploy-title">Recent container output</h3>
                  </div>
                  <button
                    type="button"
                    className="btn btn-secondary btn-sm"
                    onClick={() => {
                      void handleLoadLogs();
                    }}
                    disabled={busy === `logs-${activeKind}-${activeServiceName}`}
                  >
                    {busy === `logs-${activeKind}-${activeServiceName}`
                      ? 'Refreshing…'
                      : activeLogs.length === 0
                        ? 'Load logs'
                        : 'Refresh logs'}
                  </button>
                </div>

                {activeLogs.length === 0 ? (
                  <p className="text-muted">Logs are loaded on demand for the selected service.</p>
                ) : (
                  <pre className="service-log">{activeLogs.join('\n')}</pre>
                )}
              </div>

              {activeKind === 'postgres' ? (
                <div className="stack-md">
                  <div className="cluster justify-between align-center">
                    <div className="stack-sm">
                      <p className="eyebrow">Backups</p>
                      <h3 className="deploy-title">Snapshots and restore</h3>
                    </div>
                    <button
                      type="button"
                      className="btn btn-primary btn-sm"
                      onClick={() => {
                        void handleBackup();
                      }}
                      disabled={busy === `backup-${activeServiceName}`}
                    >
                      {busy === `backup-${activeServiceName}` ? 'Starting…' : 'Create backup'}
                    </button>
                  </div>

                  {activeBackups.length === 0 ? (
                    <p className="text-muted">No backups recorded yet.</p>
                  ) : (
                    <table className="table">
                      <thead>
                        <tr>
                          <th>ID</th>
                          <th>Created</th>
                          <th>Format</th>
                          <th>Size</th>
                          <th>Restored</th>
                          <th></th>
                        </tr>
                      </thead>
                      <tbody>
                        {activeBackups.map((backup) => (
                          <tr key={backup.id}>
                            <td className="font-mono">{backup.id.slice(0, 8)}</td>
                            <td className="font-mono">{formatDate(backup.created_at)}</td>
                            <td className="font-mono">{backup.format}</td>
                            <td className="font-mono">{formatBytes(backup.size_bytes)}</td>
                            <td className="font-mono">
                              {backup.restored_at ? formatDate(backup.restored_at) : 'No'}
                            </td>
                            <td>
                              <button
                                type="button"
                                className="btn btn-danger btn-sm"
                                onClick={() =>
                                  setRestoreTarget({ serviceName: activeServiceName, backup })
                                }
                                disabled={busy !== null}
                              >
                                Restore
                              </button>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                </div>
              ) : null}

              <div className="form-actions">
                <button
                  type="button"
                  className="btn btn-danger"
                  onClick={() => setDeleteTarget({ kind: activeKind, name: activeServiceName })}
                  disabled={busy !== null}
                >
                  Delete service
                </button>
              </div>
            </>
          )}
        </article>
      </section>

      <article className="panel stack-md">
        <div className="cluster justify-between align-center">
          <div className="stack-sm">
            <p className="eyebrow">Fleet view</p>
            <h2 className="section-title">{KIND_LABELS[activeKind]} inventory</h2>
          </div>
          <span className="text-muted">{activeServices.length} service(s)</span>
        </div>

        {activeServices.length === 0 ? (
          <p className="text-muted">Nothing provisioned for this engine yet.</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Status</th>
                <th>Container</th>
                <th>Created</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {activeServices.map((service) => (
                <tr key={service.id}>
                  <td>{service.name}</td>
                  <td>
                    <ServiceState status={service.status} />
                  </td>
                  <td className="font-mono">
                    {service.container_id ? truncateId(service.container_id) : 'pending'}
                  </td>
                  <td className="font-mono">{formatDate(service.created_at)}</td>
                  <td>
                    <button
                      type="button"
                      className="btn btn-secondary btn-sm"
                      onClick={() =>
                        setSelectedNames((current) => ({ ...current, [activeKind]: service.name }))
                      }
                    >
                      Inspect
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </article>

      <ConfirmModal
        open={deleteTarget !== null}
        title={`Delete ${deleteTarget?.name ?? 'service'}?`}
        description={
          deleteTarget
            ? `This removes the ${KIND_LABELS[deleteTarget.kind]} service ${deleteTarget.name} and its managed runtime. Linked apps will lose this dependency.`
            : ''
        }
        confirmLabel="Delete service"
        cancelLabel="Keep service"
        busy={deleteTarget !== null && busy === `delete-${deleteTarget.kind}-${deleteTarget.name}`}
        onConfirm={() => {
          void handleDelete();
        }}
        onClose={() => setDeleteTarget(null)}
      />

      <ConfirmModal
        open={restoreTarget !== null}
        title={`Restore ${restoreTarget?.serviceName ?? 'backup'}?`}
        description={
          restoreTarget
            ? `This restores ${restoreTarget.serviceName} from backup ${restoreTarget.backup.id.slice(0, 8)}. Existing data in the service may be replaced.`
            : ''
        }
        confirmLabel="Restore backup"
        cancelLabel="Cancel"
        busy={restoreTarget !== null && busy === `restore-${restoreTarget.backup.id}`}
        onConfirm={() => {
          void handleRestore();
        }}
        onClose={() => setRestoreTarget(null)}
      />
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric-card">
      <p>{label}</p>
      <strong>{value}</strong>
    </div>
  );
}

function ServiceState({ status }: { status: string }) {
  const normalized = status.toLowerCase();
  const tone =
    normalized.includes('run') || normalized.includes('ready') || normalized.includes('up')
      ? 'success'
      : normalized.includes('fail') || normalized.includes('error')
        ? 'danger'
        : 'warning';

  return <span className={`service-state service-state-${tone}`}>{status}</span>;
}

function serviceKey(kind: ManagedServiceKind, name: string): string {
  return `${kind}:${name}`;
}

function emptyKindRecord<T>(value: T): KindRecord<T> {
  return {
    postgres: value,
    redis: value,
    mysql: value,
  };
}

function chooseSelectedNames(
  current: KindRecord<string>,
  services: KindRecord<ManagedServiceSummary[]>
): KindRecord<string> {
  return {
    postgres: chooseSelectedName(current.postgres, services.postgres),
    redis: chooseSelectedName(current.redis, services.redis),
    mysql: chooseSelectedName(current.mysql, services.mysql),
  };
}

function chooseSelectedName(current: string, services: ManagedServiceSummary[]): string {
  if (current && services.some((service) => service.name === current)) {
    return current;
  }

  return services[0]?.name ?? '';
}

function formatDate(value: string): string {
  return new Date(value).toLocaleString('en-US', {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function formatBytes(value: number | null): string {
  if (value === null || value <= 0) return 'unknown';
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 * 1024 * 1024) return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  return `${(value / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function truncateId(value: string): string {
  return value.slice(0, 12);
}

function formatConnectionValue(value: string | number | boolean | null): string {
  if (value === null) return 'null';
  return String(value);
}
