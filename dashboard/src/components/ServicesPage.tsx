import { type SubmitEvent, useEffect, useMemo, useState } from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import type { ManagedServiceKind, ManagedServiceSummary, ServiceBackup } from '../lib/api';
import {
  getErrorMessage,
  getFirstQueryError,
  useCreateManagedServiceMutation,
  useDeleteManagedServiceMutation,
  useLinkManagedServiceMutation,
  useManagedServiceBackupsQuery,
  useManagedServiceDetailQuery,
  useManagedServiceLogsQuery,
  useRestoreServiceBackupMutation,
  useServicesOverviewQuery,
  useTriggerServiceBackupMutation,
  useUnlinkManagedServiceMutation,
} from '../lib/query';
import ConfirmModal from './ConfirmModal';
import ConnectScreen from './ConnectScreen';
import TableScroll from './TableScroll';

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
  kind: Extract<ManagedServiceKind, 'postgres' | 'redis'>;
  serviceName: string;
  backup: ServiceBackup;
}

type KindRecord<T> = Record<ManagedServiceKind, T>;

export default function ServicesPage() {
  const tokenAccess = useTokenAccess();

  if (tokenAccess === 'unknown') {
    return null;
  }

  if (tokenAccess === 'locked') {
    return <ConnectScreen />;
  }

  return <ServicesInner />;
}

function ServicesInner() {
  const [selectedNames, setSelectedNames] = useState<KindRecord<string>>(emptyKindRecord(''));
  const [createDrafts, setCreateDrafts] = useState<KindRecord<string>>(emptyKindRecord(''));
  const [linkDrafts, setLinkDrafts] = useState<KindRecord<string>>(emptyKindRecord(''));
  const [activeKind, setActiveKind] = useState<ManagedServiceKind>('postgres');
  const [busy, setBusy] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ServiceTarget | null>(null);
  const [restoreTarget, setRestoreTarget] = useState<RestoreTarget | null>(null);
  const overviewQuery = useServicesOverviewQuery();
  const createManagedServiceMutation = useCreateManagedServiceMutation();
  const deleteManagedServiceMutation = useDeleteManagedServiceMutation();
  const linkManagedServiceMutation = useLinkManagedServiceMutation();
  const unlinkManagedServiceMutation = useUnlinkManagedServiceMutation();
  const triggerServiceBackupMutation = useTriggerServiceBackupMutation();
  const restoreServiceBackupMutation = useRestoreServiceBackupMutation();
  const apps = overviewQuery.data?.apps ?? [];
  const servicesByKind = useMemo(
    () => overviewQuery.data?.servicesByKind ?? emptyKindRecord<ManagedServiceSummary[]>([]),
    [overviewQuery.data]
  );

  const activeServices = servicesByKind[activeKind];
  const activeServiceName = selectedNames[activeKind];
  const activeServiceKey = activeServiceName ? serviceKey(activeKind, activeServiceName) : null;
  const backupKind = supportsBackups(activeKind) ? activeKind : 'postgres';
  const detailQuery = useManagedServiceDetailQuery(activeKind, activeServiceName || '__none__', {
    enabled: Boolean(activeServiceName),
  });
  const backupsQuery = useManagedServiceBackupsQuery(backupKind, activeServiceName || '__none__', {
    enabled: Boolean(activeServiceName) && supportsBackups(activeKind),
  });
  const logsQuery = useManagedServiceLogsQuery(activeKind, activeServiceName || '__none__', 120, {
    enabled: false,
  });
  const activeDetail = activeServiceName ? (detailQuery.data ?? null) : null;
  const activeBackups =
    supportsBackups(activeKind) && activeServiceKey ? (backupsQuery.data ?? []) : [];
  const activeLogs = activeServiceKey ? (logsQuery.data ?? []) : [];
  const loading = overviewQuery.isPending;
  const queryError = getFirstQueryError(
    [overviewQuery.error, detailQuery.error, backupsQuery.error],
    null
  );
  const error = actionError ?? queryError;

  useEffect(() => {
    setSelectedNames((current) => chooseSelectedNames(current, servicesByKind));
  }, [servicesByKind]);

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

  async function handleCreate(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = createDrafts[activeKind].trim();
    if (!name) return;

    try {
      setBusy(`create-${activeKind}`);
      setActionError(null);
      setNotice(null);
      const created = await createManagedServiceMutation.mutateAsync({ kind: activeKind, name });
      setCreateDrafts((current) => ({ ...current, [activeKind]: '' }));
      setSelectedNames((current) => ({ ...current, [activeKind]: created.name }));
      setNotice(`${KIND_LABELS[activeKind]} service ${created.name} created.`);
    } catch (nextError) {
      setActionError(getErrorMessage(nextError, 'Unable to create managed service.'));
    } finally {
      setBusy(null);
    }
  }

  async function handleDelete() {
    if (!deleteTarget) return;

    try {
      setBusy(`delete-${deleteTarget.kind}-${deleteTarget.name}`);
      setActionError(null);
      setNotice(null);
      await deleteManagedServiceMutation.mutateAsync(deleteTarget);
      setDeleteTarget(null);
      setSelectedNames((current) => ({ ...current, [deleteTarget.kind]: '' }));
      setNotice(`${KIND_LABELS[deleteTarget.kind]} service ${deleteTarget.name} removed.`);
    } catch (nextError) {
      setActionError(getErrorMessage(nextError, 'Unable to delete managed service.'));
    } finally {
      setBusy(null);
    }
  }

  async function handleLink(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!activeServiceName) return;

    const appName = linkDrafts[activeKind].trim();
    if (!appName) return;

    try {
      setBusy(`link-${activeKind}-${activeServiceName}`);
      setActionError(null);
      setNotice(null);
      await linkManagedServiceMutation.mutateAsync({
        kind: activeKind,
        serviceName: activeServiceName,
        appName,
      });
      setLinkDrafts((current) => ({ ...current, [activeKind]: '' }));
      setNotice(`${activeServiceName} linked to ${appName}.`);
    } catch (nextError) {
      setActionError(getErrorMessage(nextError, 'Unable to link app.'));
    } finally {
      setBusy(null);
    }
  }

  async function handleUnlink(appName: string) {
    if (!activeServiceName) return;

    try {
      setBusy(`unlink-${activeKind}-${activeServiceName}-${appName}`);
      setActionError(null);
      setNotice(null);
      await unlinkManagedServiceMutation.mutateAsync({
        kind: activeKind,
        serviceName: activeServiceName,
        appName,
      });
      setNotice(`${appName} unlinked from ${activeServiceName}.`);
    } catch (nextError) {
      setActionError(getErrorMessage(nextError, 'Unable to unlink app.'));
    } finally {
      setBusy(null);
    }
  }

  async function handleLoadLogs() {
    if (!activeServiceName) return;

    try {
      setBusy(`logs-${activeKind}-${activeServiceName}`);
      setActionError(null);
      const result = await logsQuery.refetch();
      if (result.error) {
        throw result.error;
      }
    } catch (nextError) {
      setActionError(getErrorMessage(nextError, 'Unable to load service logs.'));
    } finally {
      setBusy(null);
    }
  }

  async function handleBackup() {
    if (!activeServiceName || !supportsBackups(activeKind)) return;

    try {
      setBusy(`backup-${activeServiceName}`);
      setActionError(null);
      setNotice(null);
      await triggerServiceBackupMutation.mutateAsync({ kind: activeKind, name: activeServiceName });
      setNotice(`Backup started for ${activeServiceName}.`);
    } catch (nextError) {
      setActionError(getErrorMessage(nextError, 'Unable to create backup.'));
    } finally {
      setBusy(null);
    }
  }

  async function handleRestore() {
    if (!restoreTarget) return;

    try {
      setBusy(`restore-${restoreTarget.backup.id}`);
      setActionError(null);
      setNotice(null);
      await restoreServiceBackupMutation.mutateAsync({
        kind: restoreTarget.kind,
        name: restoreTarget.serviceName,
        backupId: restoreTarget.backup.id,
      });
      setRestoreTarget(null);
      setNotice(
        `Restore started for ${restoreTarget.serviceName} using backup ${restoreTarget.backup.id.slice(0, 8)}.`
      );
    } catch (nextError) {
      setActionError(getErrorMessage(nextError, 'Unable to restore backup.'));
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return null;
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
          ) : detailQuery.isPending && !activeDetail ? (
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
                  <TableScroll>
                    <table className="table">
                      <thead>
                        <tr>
                          <th>App</th>
                          <th>Env key</th>
                          <th />
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
                  </TableScroll>
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

              {supportsBackups(activeKind) ? (
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
                    <TableScroll>
                      <table className="table">
                        <thead>
                          <tr>
                            <th>ID</th>
                            <th>Created</th>
                            <th>Format</th>
                            <th>Size</th>
                            <th>Restored</th>
                            <th />
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
                                    setRestoreTarget({
                                      kind: activeKind,
                                      serviceName: activeServiceName,
                                      backup,
                                    })
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
                    </TableScroll>
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
          <TableScroll>
            <table className="table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Status</th>
                  <th>Container</th>
                  <th>Created</th>
                  <th />
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
                          setSelectedNames((current) => ({
                            ...current,
                            [activeKind]: service.name,
                          }))
                        }
                      >
                        Inspect
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

function supportsBackups(
  kind: ManagedServiceKind
): kind is Extract<ManagedServiceKind, 'postgres' | 'redis'> {
  return kind === 'postgres' || kind === 'redis';
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
