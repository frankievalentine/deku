import { useEffect, useMemo, useState } from 'react';
import {
  fetchApp,
  fetchDeployments,
  fetchEvents,
  getToken,
  type App,
  type Deployment,
  type EventRecord,
} from '../lib/api';
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
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const query = new URLSearchParams(window.location.search);
    setAppName(query.get('app') ?? '');
    setSelectedId(query.get('id') ?? '');
  }, []);

  useEffect(() => {
    if (!appName) {
      setLoading(false);
      return;
    }

    async function load() {
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
    }

    void load();
  }, [appName]);

  const selectedDeployment = useMemo(
    () => deployments.find((deployment) => deployment.id === selectedId) ?? deployments[0] ?? null,
    [deployments, selectedId]
  );

  const matchingEvents = useMemo(() => {
    if (!selectedDeployment) return [];
    return events.filter((event) => {
      const payload = parsePayload(event.payload);
      return payload?.deploy_id === selectedDeployment.id;
    });
  }, [events, selectedDeployment]);

  if (!appName) {
    return (
      <div className="panel empty-state">
        <h1 className="section-title">Select a deployment</h1>
        <p className="page-copy">
          Open a deployment from an app detail page to inspect its status and related lifecycle
          events.
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

  return (
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
          <Metric label="Image" value={selectedDeployment.image_tag ? 'Tagged' : 'Pending'} />
          <Metric label="Created" value={formatCompact(selectedDeployment.created_at)} />
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
              {deployments.map((deployment) => (
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
            <p className="eyebrow">Lifecycle notes</p>
            <h2 className="section-title">Tagged events</h2>
          </div>
          {matchingEvents.length === 0 ? (
            <p className="text-muted">
              No deploy-id tagged events were recorded for this deployment. The current daemon event
              model still emits some build lines without a deployment identifier.
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

function parsePayload(payload: string | null): Record<string, string> | null {
  if (!payload) return null;
  try {
    return JSON.parse(payload) as Record<string, string>;
  } catch {
    return null;
  }
}

function summarisePayload(payload: string | null): string {
  const parsed = parsePayload(payload);
  if (!parsed) return 'No structured payload';
  return Object.entries(parsed)
    .map(([key, value]) => `${key}=${value}`)
    .join(' | ');
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
