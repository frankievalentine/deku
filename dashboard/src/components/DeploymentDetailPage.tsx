import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  type App,
  type Deployment,
  type EventRecord,
  fetchApp,
  fetchDeployments,
  fetchEvents,
  getToken,
  triggerRollback,
} from '../lib/api';
import ConfirmModal from './ConfirmModal';
import ConnectScreen from './ConnectScreen';
import StatusBadge from './StatusBadge';

export default function DeploymentDetailPage() {
  const [hasToken, setHasToken] = useState(() => Boolean(getToken()));

  if (!hasToken) {
    return <ConnectScreen onConnected={() => setHasToken(true)} />;
  }

  return <DeploymentDetailInner />;
}

function DeploymentDetailInner() {
  const [appName, setAppName] = useState('');
  const [selectedId, setSelectedId] = useState('');
  const [app, setApp] = useState<App | null>(null);
  const [deployments, setDeployments] = useState<Deployment[]>([]);
  const [events, setEvents] = useState<EventRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [confirmRollback, setConfirmRollback] = useState(false);

  useEffect(() => {
    const query = new URLSearchParams(window.location.search);
    setAppName(query.get('app') ?? '');
    setSelectedId(query.get('id') ?? '');
  }, []);

  const load = useCallback(async () => {
    if (!appName) {
      setLoading(false);
      return;
    }

    try {
      setLoading(true);
      setError(null);
      const nextApp = await fetchApp(appName);
      const [nextDeployments, nextEvents] = await Promise.all([
        fetchDeployments(appName),
        fetchEvents(nextApp.id),
      ]);
      setApp(nextApp);
      setDeployments(nextDeployments);
      setEvents(nextEvents);
      if (!selectedId && nextDeployments[0]) {
        setSelectedId(nextDeployments[0].id);
      }
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to load deployment detail.'
      );
    } finally {
      setLoading(false);
    }
  }, [appName, selectedId]);

  useEffect(() => {
    if (!appName) {
      setLoading(false);
      return;
    }

    void load();
  }, [appName, load]);

  const sortedDeployments = useMemo(
    () =>
      [...deployments].sort(
        (left, right) => Date.parse(right.created_at) - Date.parse(left.created_at)
      ),
    [deployments]
  );

  const selectedDeployment = useMemo(
    () =>
      sortedDeployments.find((deployment) => deployment.id === selectedId) ??
      sortedDeployments[0] ??
      null,
    [selectedId, sortedDeployments]
  );

  const latestDeployment = sortedDeployments[0] ?? null;

  const matchingEvents = useMemo(() => {
    if (!selectedDeployment) return [];
    return events.filter((event) => eventTargetsDeployment(event, selectedDeployment.id));
  }, [events, selectedDeployment]);

  const consoleEntries = useMemo(
    () => matchingEvents.map((event) => formatConsoleEntry(event)).filter(Boolean),
    [matchingEvents]
  );

  const lifecycleItems = useMemo(
    () => (selectedDeployment ? buildLifecycle(selectedDeployment, matchingEvents) : []),
    [matchingEvents, selectedDeployment]
  );

  async function handleRollback() {
    if (!selectedDeployment) return;

    try {
      setBusy('rollback');
      setError(null);
      setNotice(null);
      await triggerRollback(appName, selectedDeployment.id);
      setConfirmRollback(false);
      await load();
      setNotice(`Rollback started to deployment ${selectedDeployment.id.slice(0, 8)}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to start rollback.');
    } finally {
      setBusy(null);
    }
  }

  if (!appName) {
    return (
      <div className="panel empty-state">
        <h1 className="section-title">Select a deployment</h1>
        <p className="page-copy">
          Open a deployment from an app detail page to inspect its status, source details, rollback
          targetability, and related lifecycle events.
        </p>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <span className="loading-spinner" />
        <span>Loading deployment history…</span>
      </div>
    );
  }

  if (error || !app || !selectedDeployment) {
    return (
      <div className="panel error-state">
        Failed to load deployment detail: {error ?? 'No deployment selected.'}
      </div>
    );
  }

  const canRollback =
    Boolean(latestDeployment) &&
    latestDeployment?.id !== selectedDeployment.id &&
    Boolean(selectedDeployment.image_tag);

  return (
    <>
      <div className="stack-lg">
        <section className="hero-panel">
          <div className="stack-md">
            <p className="eyebrow">Deployment detail</p>
            <div className="cluster justify-between align-start">
              <div className="stack-sm">
                <h1 className="page-title">{app.name}</h1>
                <p className="page-copy">
                  Deployment {selectedDeployment.id.slice(0, 8)} using {selectedDeployment.builder}.
                </p>
              </div>
              <StatusBadge status={selectedDeployment.status} />
            </div>
          </div>
          <div className="metrics-grid">
            <Metric label="Builder" value={selectedDeployment.builder} />
            <Metric
              label="Artifact"
              value={selectedDeployment.image_tag ? 'Resolved' : 'Pending'}
            />
            <Metric label="Events" value={String(matchingEvents.length)} />
            <Metric
              label="Finished"
              value={
                selectedDeployment.finished_at
                  ? formatCompact(selectedDeployment.finished_at)
                  : 'Open'
              }
            />
          </div>
        </section>

        {notice ? <p className="callout callout-success">{notice}</p> : null}
        {error ? <p className="callout callout-danger">{error}</p> : null}

        <section className="panel-grid">
          <article className="panel stack-md">
            <div className="stack-sm">
              <p className="eyebrow">Artifact</p>
              <h2 className="section-title">Source and rollout</h2>
            </div>

            <dl className="data-grid">
              <div>
                <dt>Deployment ID</dt>
                <dd className="font-mono">{selectedDeployment.id}</dd>
              </div>
              <div>
                <dt>Builder</dt>
                <dd>{selectedDeployment.builder}</dd>
              </div>
              <div>
                <dt>Status</dt>
                <dd>{selectedDeployment.status}</dd>
              </div>
              <div>
                <dt>Image tag</dt>
                <dd className="font-mono">{selectedDeployment.image_tag ?? 'Not resolved yet'}</dd>
              </div>
              <div>
                <dt>Created</dt>
                <dd className="font-mono">{formatDate(selectedDeployment.created_at)}</dd>
              </div>
              <div>
                <dt>Finished</dt>
                <dd className="font-mono">
                  {selectedDeployment.finished_at
                    ? formatDate(selectedDeployment.finished_at)
                    : 'Still running'}
                </dd>
              </div>
              <div>
                <dt>Duration</dt>
                <dd className="font-mono">{formatDuration(selectedDeployment)}</dd>
              </div>
              <div>
                <dt>Rollback target</dt>
                <dd>{canRollback ? 'Yes' : 'No'}</dd>
              </div>
            </dl>

            {canRollback ? (
              <div className="form-actions">
                <button
                  type="button"
                  className="btn btn-danger"
                  onClick={() => setConfirmRollback(true)}
                  disabled={busy !== null}
                >
                  Roll back to this deployment
                </button>
              </div>
            ) : (
              <p className="text-muted">
                Select an older image-backed deployment to trigger a targeted rollback from this
                page.
              </p>
            )}
          </article>

          <article className="panel stack-md">
            <div className="stack-sm">
              <p className="eyebrow">Lifecycle</p>
              <h2 className="section-title">Phase progression</h2>
            </div>

            <div className="service-list">
              {lifecycleItems.map((item) => (
                <div key={item.label} className="service-list-item">
                  <div className="cluster justify-between align-start">
                    <strong>{item.label}</strong>
                    <ServiceState status={item.state} />
                  </div>
                  <span className="service-list-meta">{item.detail}</span>
                </div>
              ))}
            </div>
          </article>
        </section>

        <article className="panel stack-md">
          <div className="cluster justify-between align-start">
            <div className="stack-sm">
              <p className="eyebrow">Deploy console</p>
              <h2 className="section-title">Structured event stream</h2>
            </div>
            <span className="inventory-summary">{consoleEntries.length} lines</span>
          </div>

          {consoleEntries.length === 0 ? (
            <p className="text-muted">
              No deploy-tagged console lines were recorded for this deployment. Some build output is
              still emitted without a deployment identifier.
            </p>
          ) : (
            <pre className="service-log">{consoleEntries.join('\n')}</pre>
          )}
        </article>

        <section className="panel-grid">
          <article className="panel stack-md">
            <div className="stack-sm">
              <p className="eyebrow">Deployment history</p>
              <h2 className="section-title">All deployments</h2>
            </div>
            <table className="table">
              <thead>
                <tr>
                  <th>ID</th>
                  <th>Status</th>
                  <th>Builder</th>
                  <th>Created</th>
                </tr>
              </thead>
              <tbody>
                {sortedDeployments.map((deployment) => (
                  <tr key={deployment.id}>
                    <td>
                      <a
                        href={`/deployments?app=${encodeURIComponent(app.name)}&id=${deployment.id}`}
                        className="font-mono"
                      >
                        {deployment.id.slice(0, 8)}
                      </a>
                    </td>
                    <td>
                      <StatusBadge status={deployment.status} size="sm" />
                    </td>
                    <td className="font-mono">{deployment.builder}</td>
                    <td className="font-mono">{formatDate(deployment.created_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </article>

          <article className="panel stack-md">
            <div className="stack-sm">
              <p className="eyebrow">Tagged events</p>
              <h2 className="section-title">Payload detail</h2>
            </div>
            {matchingEvents.length === 0 ? (
              <p className="text-muted">
                No deployment-tagged events were recorded for this deployment.
              </p>
            ) : (
              <table className="table">
                <thead>
                  <tr>
                    <th>When</th>
                    <th>Type</th>
                    <th>Payload</th>
                  </tr>
                </thead>
                <tbody>
                  {matchingEvents.map((event) => (
                    <tr key={event.id}>
                      <td className="font-mono">{formatDate(event.created_at)}</td>
                      <td className="font-mono">{event.event_type}</td>
                      <td className="font-mono">{summarisePayload(event.payload)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </article>
        </section>
      </div>

      <ConfirmModal
        open={confirmRollback}
        title="Roll back to this deployment?"
        description={`Deku will redeploy ${selectedDeployment.image_tag ?? 'the selected image'} for ${app.name}.`}
        confirmLabel="Start rollback"
        busy={busy === 'rollback'}
        onClose={() => {
          if (busy !== 'rollback') setConfirmRollback(false);
        }}
        onConfirm={() => {
          void handleRollback();
        }}
      />
    </>
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

function parsePayload(payload: string | null): Record<string, unknown> | null {
  if (!payload) return null;
  try {
    return JSON.parse(payload) as Record<string, unknown>;
  } catch {
    return null;
  }
}

function summarisePayload(payload: string | null): string {
  const parsed = parsePayload(payload);
  if (!parsed) return 'No structured payload';
  return Object.entries(parsed)
    .map(([key, value]) => `${key}=${String(value)}`)
    .join(' | ');
}

function eventTargetsDeployment(event: EventRecord, deploymentId: string): boolean {
  const payload = parsePayload(event.payload);
  return payload?.deploy_id === deploymentId || payload?.to_deploy_id === deploymentId;
}

function formatConsoleEntry(event: EventRecord): string | null {
  const payload = parsePayload(event.payload);
  const prefix = `[${formatDate(event.created_at)}]`;

  switch (event.event_type) {
    case 'build.log':
    case 'deploy.release.log':
      return payload?.line ? `${prefix} ${String(payload.line)}` : null;
    case 'deploy.health_checking':
      return `${prefix} running health checks on port ${String(payload?.port ?? '-')}`;
    case 'deploy.rollback':
      return `${prefix} rolling back deploy: ${String(payload?.reason ?? 'unspecified')}`;
    case 'deploy.rollback.started':
      return `${prefix} rollback started to deployment ${String(payload?.to_deploy_id ?? '-')}`;
    case 'deploy.live':
      return `${prefix} deploy live: ${String(payload?.url ?? 'no url emitted')}`;
    case 'deploy.failed':
      return `${prefix} deploy failed: ${String(payload?.error ?? 'unknown error')}`;
    case 'deploy.complete':
      return `${prefix} deploy completed`;
    default:
      return `${prefix} ${event.event_type} ${summarisePayload(event.payload)}`;
  }
}

function buildLifecycle(deployment: Deployment, events: EventRecord[]) {
  const order = ['pending', 'building', 'built', 'deploying', 'health_checking', 'live'] as const;
  const normalizedStatus = order.includes(deployment.status as (typeof order)[number])
    ? deployment.status
    : 'pending';
  const currentIndex = order.indexOf(normalizedStatus as (typeof order)[number]);
  const healthEvent = events.find((event) => event.event_type === 'deploy.health_checking');
  const liveEvent = events.find((event) => event.event_type === 'deploy.live');
  const rollbackEvent = events.find(
    (event) =>
      event.event_type === 'deploy.rollback' || event.event_type === 'deploy.rollback.started'
  );
  const failedEvent = events.find((event) => event.event_type === 'deploy.failed');

  return [
    {
      label: 'Queued',
      state: 'complete',
      detail: 'Deployment row created and waiting for work.',
    },
    {
      label: 'Build',
      state: currentIndex > 1 ? 'complete' : currentIndex === 1 ? 'active' : 'pending',
      detail: `Builder: ${deployment.builder}`,
    },
    {
      label: 'Deploy',
      state: currentIndex > 3 ? 'complete' : currentIndex === 3 ? 'active' : 'pending',
      detail: deployment.image_tag
        ? `Image ${deployment.image_tag}`
        : 'Image tag not recorded yet.',
    },
    {
      label: 'Health check',
      state:
        deployment.status === 'failed'
          ? 'failed'
          : currentIndex > 4
            ? 'complete'
            : currentIndex === 4
              ? 'active'
              : 'pending',
      detail: healthEvent
        ? summarisePayload(healthEvent.payload)
        : 'No health-check event was tagged for this deployment.',
    },
    {
      label: 'Live',
      state:
        deployment.status === 'live'
          ? 'complete'
          : deployment.status === 'rolled_back'
            ? 'rolled_back'
            : deployment.status === 'failed'
              ? 'failed'
              : 'pending',
      detail: liveEvent
        ? summarisePayload(liveEvent.payload)
        : failedEvent
          ? summarisePayload(failedEvent.payload)
          : rollbackEvent
            ? summarisePayload(rollbackEvent.payload)
            : 'No live event was tagged for this deployment.',
    },
  ];
}

function formatDate(value: string): string {
  return new Date(value).toLocaleString('en-US', {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function formatCompact(value: string): string {
  return new Date(value).toLocaleTimeString('en-US', {
    hour: '2-digit',
    minute: '2-digit',
  });
}

function formatDuration(deployment: Deployment): string {
  const end = deployment.finished_at ? Date.parse(deployment.finished_at) : Date.now();
  const start = Date.parse(deployment.created_at);
  if (Number.isNaN(start) || Number.isNaN(end) || end < start) {
    return 'Unavailable';
  }

  const totalSeconds = Math.round((end - start) / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
}

function ServiceState({ status }: { status: string }) {
  const normalized = status.toLowerCase();
  const tone =
    normalized === 'complete'
      ? 'service-state-success'
      : normalized === 'active'
        ? 'service-state-warning'
        : normalized === 'failed' || normalized === 'rolled_back'
          ? 'service-state-danger'
          : 'service-state-warning';

  return <span className={`service-state ${tone}`}>{status}</span>;
}
