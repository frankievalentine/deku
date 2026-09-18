import { type SubmitEvent, useEffect, useMemo, useRef, useState } from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import {
  MANAGED_SERVICE_KINDS,
  type ManagedServiceKind,
  type ManagedServiceSummary,
  type ServiceBackup,
} from '../lib/api';
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
import { SERVICE_KIND_DESCRIPTIONS, SERVICE_KIND_LABELS } from '../lib/service-meta';
import ConfirmModal from './ConfirmModal';
import ConnectScreen from './ConnectScreen';
import ServiceStateBadge from './ServiceStateBadge';
import TableScroll from './TableScroll';
import Spinner from './Spinner';

const SERVICE_KINDS: ManagedServiceKind[] = [...MANAGED_SERVICE_KINDS];

const KIND_LABELS = SERVICE_KIND_LABELS;

const KIND_DESCRIPTIONS = SERVICE_KIND_DESCRIPTIONS;

/// Every engine Deku provisions has an object-store backup implementation.
const BACKUP_KINDS = new Set<ManagedServiceKind>(MANAGED_SERVICE_KINDS);

interface ServiceTarget {
  kind: ManagedServiceKind;
  name: string;
}

interface RestoreTarget {
  kind: ManagedServiceKind;
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
  const [createError, setCreateError] = useState<string | null>(null);
  const [linkError, setLinkError] = useState<string | null>(null);
  const [activeKind, setActiveKind] = useState<ManagedServiceKind>('postgres');
  const [busy, setBusy] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ServiceTarget | null>(null);
  const [unlinkTarget, setUnlinkTarget] = useState<string | null>(null);
  const [restoreTarget, setRestoreTarget] = useState<RestoreTarget | null>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const linkSelect = useRef<HTMLSelectElement>(null);
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
  const detailQuery = useManagedServiceDetailQuery(activeKind, activeServiceName || '__none__', {
    enabled: Boolean(activeServiceName),
  });
  const backupsQuery = useManagedServiceBackupsQuery(activeKind, activeServiceName || '__none__', {
    enabled: Boolean(activeServiceName),
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
    if (!name) {
      setCreateError(`Enter a name for the new ${KIND_LABELS[activeKind]} service.`);
      nameInput.current?.focus();
      return;
    }

    try {
      setBusy(`create-${activeKind}`);
      setActionError(null);
      setNotice(null);
      setCreateError(null);
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
    if (!appName) {
      setLinkError('Select an app to link.');
      linkSelect.current?.focus();
      return;
    }

    try {
      setBusy(`link-${activeKind}-${activeServiceName}`);
      setActionError(null);
      setNotice(null);
      setLinkError(null);
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

  async function handleUnlink() {
    const appName = unlinkTarget;
    if (!appName || !activeServiceName) return;

    try {
      setBusy(`unlink-${activeKind}-${activeServiceName}-${appName}`);
      setActionError(null);
      setNotice(null);
      await unlinkManagedServiceMutation.mutateAsync({
        kind: activeKind,
        serviceName: activeServiceName,
        appName,
      });
      setUnlinkTarget(null);
      setNotice(`${appName} unlinked from ${activeServiceName}.`);
    } catch (nextError) {
      setActionError(getErrorMessage(nextError, 'Unable to unlink app.'));
      setUnlinkTarget(null);
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
    return (
      <div className="panel loading-state">
        <Spinner />
        <span>Loading managed services…</span>
      </div>
    );
  }

  if (queryError && overviewQuery.data === undefined) {
    return (
      <div className="panel error-state">
        <p className="text-danger" role="alert">
          {queryError}
        </p>
        <button
          type="button"
          className="btn btn-secondary"
          onClick={() => void overviewQuery.refetch()}
        >
          Retry loading services
        </button>
      </div>
    );
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <h1 className="page-title">Managed services</h1>
          <p className="page-copy">
            Provision Postgres, MySQL, MariaDB, Redis, or MongoDB, then connect them to apps.
          </p>
        </div>
        <div className="metrics-grid">
          {SERVICE_KINDS.map((kind) => (
            <Metric
              key={kind}
              label={KIND_LABELS[kind]}
              value={String(servicesByKind[kind].length)}
            />
          ))}
          <Metric label="Total" value={String(totalServices)} />
        </div>
      </section>

      <section className="panel stack-md">
        <h2 className="section-title">Service engines</h2>

        <fieldset className="segment-control min-w-0" aria-label="Service engines">
          {SERVICE_KINDS.map((kind) => (
            <button
              key={kind}
              type="button"
              aria-pressed={activeKind === kind}
              className={`segment-button${activeKind === kind ? ' is-selected' : ''}`}
              onClick={() => {
                setActiveKind(kind);
                setCreateError(null);
                setLinkError(null);
              }}
            >
              <span>{KIND_LABELS[kind]}</span>
              <strong>{servicesByKind[kind].length}</strong>
            </button>
          ))}
        </fieldset>

        <p className="text-muted">{KIND_DESCRIPTIONS[activeKind]}</p>

        {notice ? (
          <p className="callout callout-success" role="status">
            {notice}
          </p>
        ) : null}
        {error ? (
          <p className="callout callout-danger" role="alert">
            {error}
          </p>
        ) : null}
      </section>

      <section className="panel-grid">
        <article className="panel stack-md">
          <h2 className="section-title">Create {KIND_LABELS[activeKind]}</h2>

          <form onSubmit={handleCreate} className="stack-md" noValidate>
            <div className="form-group">
              <label className="form-label" htmlFor="service-name">
                Service name
              </label>
              <input
                id="service-name"
                ref={nameInput}
                className="input"
                value={createDrafts[activeKind]}
                onChange={(event) => {
                  setCreateDrafts((current) => ({ ...current, [activeKind]: event.target.value }));
                  if (createError) setCreateError(null);
                }}
                placeholder={`${activeKind}-main`}
                autoComplete="off"
                required
                aria-invalid={createError ? true : undefined}
                aria-describedby={createError ? 'service-name-error' : undefined}
              />
              {createError ? (
                <p id="service-name-error" className="text-danger">
                  {createError}
                </p>
              ) : null}
            </div>
            <div className="form-actions">
              <button
                className="btn btn-primary"
                type="submit"
                disabled={busy === `create-${activeKind}`}
              >
                {busy === `create-${activeKind}` ? <span className="loading-spinner" /> : null}
                <span>Create {KIND_LABELS[activeKind]}</span>
              </button>
            </div>
          </form>

          <h3 className="deploy-title">Existing {KIND_LABELS[activeKind]} services</h3>

          {activeServices.length === 0 ? (
            <p className="text-muted">
              No {KIND_LABELS[activeKind]} services yet. Create one with the form above.
            </p>
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
                      <ServiceStateBadge status={service.status} />
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
              <h2 className="section-title">No service selected</h2>
              <p className="page-copy">
                Select a {KIND_LABELS[activeKind]} service to see its connection details, links,
                logs, and backups.
              </p>
            </>
          ) : detailQuery.isPending && !activeDetail ? (
            <div className="loading-state service-detail-loading">
              <Spinner />
              <span>Loading {activeServiceName}…</span>
            </div>
          ) : !activeDetail ? (
            <>
              <h2 className="section-title">Service unavailable</h2>
              <p className="text-danger" role="alert">
                Unable to load details for {activeServiceName}. Refresh the page to try again.
              </p>
            </>
          ) : (
            <>
              <div className="cluster justify-between align-start">
                <div className="stack-sm">
                  <h2 className="section-title">{activeDetail.name}</h2>
                  <p className="page-copy">
                    {KIND_LABELS[activeKind]} service created {formatDate(activeDetail.created_at)}.
                  </p>
                </div>
                <ServiceStateBadge status={activeDetail.status} />
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

              <h3 className="deploy-title">Connection values</h3>

              <dl className="connection-grid">
                {Object.entries(activeDetail.connection).map(([key, value]) => (
                  <div key={key} className="connection-card">
                    <dt>{key}</dt>
                    <dd className="font-mono">{formatConnectionValue(value)}</dd>
                  </div>
                ))}
              </dl>

              <div className="stack-md">
                <h3 className="deploy-title">Attached apps</h3>

                <form onSubmit={handleLink} className="stack-md" noValidate>
                  <div className="form-group">
                    <label className="form-label" htmlFor="link-app">
                      Link to app
                    </label>
                    <select
                      id="link-app"
                      ref={linkSelect}
                      className="input"
                      value={linkDrafts[activeKind]}
                      onChange={(event) => {
                        setLinkDrafts((current) => ({
                          ...current,
                          [activeKind]: event.target.value,
                        }));
                        if (linkError) setLinkError(null);
                      }}
                      required
                      aria-invalid={linkError ? true : undefined}
                      aria-describedby={linkError ? 'link-app-error' : undefined}
                    >
                      <option value="">Select app</option>
                      {availableApps.map((app) => (
                        <option key={app.id} value={app.name}>
                          {app.name}
                        </option>
                      ))}
                    </select>
                    {linkError ? (
                      <p id="link-app-error" className="text-danger">
                        {linkError}
                      </p>
                    ) : null}
                  </div>
                  <div className="form-actions">
                    <button
                      className="btn btn-secondary"
                      type="submit"
                      disabled={busy === `link-${activeKind}-${activeServiceName}`}
                    >
                      {busy === `link-${activeKind}-${activeServiceName}` ? (
                        <span className="loading-spinner" />
                      ) : null}
                      <span>Link app</span>
                    </button>
                  </div>
                </form>

                {activeDetail.links.length === 0 ? (
                  <p className="text-muted">
                    No apps use this service yet. Link one above to inject its connection values.
                  </p>
                ) : (
                  <TableScroll>
                    <table className="table">
                      <caption className="sr-only">Apps linked to this service</caption>
                      <thead>
                        <tr>
                          <th scope="col">App</th>
                          <th scope="col">Env key</th>
                          <th scope="col">
                            <span className="sr-only">Actions</span>
                          </th>
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
                                aria-label={`Unlink ${link.name}`}
                                disabled={busy !== null}
                                onClick={() => setUnlinkTarget(link.name)}
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
                  <h3 className="deploy-title">Recent container output</h3>
                  <button
                    type="button"
                    className="btn btn-secondary btn-sm"
                    onClick={() => {
                      void handleLoadLogs();
                    }}
                    disabled={busy === `logs-${activeKind}-${activeServiceName}`}
                  >
                    {busy === `logs-${activeKind}-${activeServiceName}` ? (
                      <span className="loading-spinner" />
                    ) : null}
                    <span>{activeLogs.length === 0 ? 'Load logs' : 'Refresh logs'}</span>
                  </button>
                </div>

                {activeLogs.length === 0 ? (
                  <p className="text-muted">
                    Logs are not loaded automatically. Select Load logs to fetch the last 120 lines.
                  </p>
                ) : (
                  <pre className="service-log">{activeLogs.join('\n')}</pre>
                )}
              </div>

              {supportsBackups(activeKind) ? (
                <div className="stack-md">
                  <div className="cluster justify-between align-center">
                    <h3 className="deploy-title">Snapshots and restore</h3>
                    <button
                      type="button"
                      className="btn btn-primary btn-sm"
                      onClick={() => {
                        void handleBackup();
                      }}
                      disabled={busy === `backup-${activeServiceName}`}
                    >
                      {busy === `backup-${activeServiceName}` ? (
                        <span className="loading-spinner" />
                      ) : null}
                      <span>Create backup</span>
                    </button>
                  </div>

                  {activeBackups.length === 0 ? (
                    <p className="text-muted">
                      No backups yet. Create one before making a change you might need to roll back.
                    </p>
                  ) : (
                    <TableScroll>
                      <table className="table">
                        <caption className="sr-only">Backups for {activeServiceName}</caption>
                        <thead>
                          <tr>
                            <th scope="col">ID</th>
                            <th scope="col">Created</th>
                            <th scope="col">Format</th>
                            <th scope="col">Size</th>
                            <th scope="col">Encryption</th>
                            <th scope="col">Restored</th>
                            <th scope="col">
                              <span className="sr-only">Actions</span>
                            </th>
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
                                {backup.encryption === 'none' ? 'None' : backup.encryption}
                              </td>
                              <td className="font-mono">
                                {backup.restored_at ? formatDate(backup.restored_at) : 'No'}
                              </td>
                              <td>
                                <button
                                  type="button"
                                  className="btn btn-danger btn-sm"
                                  aria-label={`Restore backup ${backup.id.slice(0, 8)}`}
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
          <h2 className="section-title">{KIND_LABELS[activeKind]} inventory</h2>
          <span className="text-muted">
            {activeServices.length} {activeServices.length === 1 ? 'service' : 'services'}
          </span>
        </div>

        {activeServices.length === 0 ? (
          <p className="text-muted">
            No {KIND_LABELS[activeKind]} services yet. Create one to see it here.
          </p>
        ) : (
          <TableScroll>
            <table className="table">
              <caption className="sr-only">
                Every {KIND_LABELS[activeKind]} service on this host
              </caption>
              <thead>
                <tr>
                  <th scope="col">Name</th>
                  <th scope="col">Status</th>
                  <th scope="col">Container</th>
                  <th scope="col">Created</th>
                  <th scope="col">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {activeServices.map((service) => (
                  <tr key={service.id}>
                    <td>{service.name}</td>
                    <td>
                      <ServiceStateBadge status={service.status} />
                    </td>
                    <td className="font-mono">
                      {service.container_id ? truncateId(service.container_id) : 'pending'}
                    </td>
                    <td className="font-mono">{formatDate(service.created_at)}</td>
                    <td>
                      <button
                        type="button"
                        className="btn btn-secondary btn-sm"
                        aria-label={`Inspect ${service.name}`}
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
        onClose={() => {
          if (busy === null) setDeleteTarget(null);
        }}
      />

      <ConfirmModal
        open={unlinkTarget !== null}
        title={`Unlink ${unlinkTarget ?? 'app'}?`}
        description={`This removes the managed connection variables for ${activeServiceName || 'this service'} from ${unlinkTarget ?? 'the app'}. The app keeps running without them.`}
        confirmLabel="Unlink app"
        cancelLabel="Keep link"
        busy={
          unlinkTarget !== null &&
          busy === `unlink-${activeKind}-${activeServiceName}-${unlinkTarget}`
        }
        onConfirm={() => {
          void handleUnlink();
        }}
        onClose={() => {
          if (busy === null) setUnlinkTarget(null);
        }}
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
        onClose={() => {
          if (busy === null) setRestoreTarget(null);
        }}
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

function serviceKey(kind: ManagedServiceKind, name: string): string {
  return `${kind}:${name}`;
}

function supportsBackups(kind: ManagedServiceKind): boolean {
  return BACKUP_KINDS.has(kind);
}

function emptyKindRecord<T>(value: T): KindRecord<T> {
  return {
    postgres: value,
    redis: value,
    mysql: value,
    mariadb: value,
    mongodb: value,
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
    mariadb: chooseSelectedName(current.mariadb, services.mariadb),
    mongodb: chooseSelectedName(current.mongodb, services.mongodb),
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
