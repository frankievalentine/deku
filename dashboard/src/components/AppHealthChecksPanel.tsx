import { useState } from 'react';
import { type AppHealthCheck, runAppChecks } from '../lib/api';
import Spinner from './Spinner';

interface AppHealthChecksPanelProps {
  appName: string;
  locked: boolean;
}

const STATUS_LABELS: Record<string, string> = {
  ok: 'Healthy',
  warn: 'Needs attention',
  fail: 'Failing',
  error: 'Failing',
};

export default function AppHealthChecksPanel({ appName, locked }: AppHealthChecksPanelProps) {
  const [result, setResult] = useState<AppHealthCheck | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleRun() {
    try {
      setRunning(true);
      setError(null);
      setResult(await runAppChecks(appName));
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to run the health check.');
    } finally {
      setRunning(false);
    }
  }

  const status = result?.status ?? null;

  return (
    <article className="panel stack-md">
      <div className="panel-heading">
        <div className="stack-sm panel-heading-copy">
          <p className="eyebrow">Health</p>
          <h2 className="section-title">Check the running app</h2>
          <p className="page-copy">
            Probe the app through its published port and confirm its containers and routes are
            working. Run it after a deploy to be sure traffic is being served.
          </p>
        </div>
        <span className="inventory-summary">
          {status ? (STATUS_LABELS[status] ?? status) : 'Not run yet'}
        </span>
      </div>

      {error ? <p className="callout callout-danger">{error}</p> : null}

      <div className="form-actions">
        <button
          className="btn btn-primary"
          type="button"
          onClick={() => {
            void handleRun();
          }}
          disabled={locked || running}
        >
          {running ? <span className="loading-spinner" /> : null}
          <span>{running ? 'Checking…' : 'Run health check'}</span>
        </button>
      </div>

      {result ? (
        <div className="stack-md">
          {result.issues.length > 0 ? (
            <div className="stack-sm">
              <h3 className="deploy-title">What needs attention</h3>
              <ul className="issue-list">
                {result.issues.map((issue) => (
                  <li key={issue} className="issue-item">
                    {issue}
                  </li>
                ))}
              </ul>
            </div>
          ) : (
            <p className="callout callout-success">No problems found.</p>
          )}

          <div className="stack-sm">
            <h3 className="deploy-title">Checks</h3>
            {result.probes.length === 0 ? (
              <p className="text-muted">No probes ran. The app may not have a published port.</p>
            ) : (
              <dl className="data-grid">
                {result.probes.map((probe) => (
                  <div key={probe.target}>
                    <dt>{probe.target}</dt>
                    <dd>
                      {probe.ok ? 'Reachable' : 'Unreachable'}
                      {probe.status_code ? ` - HTTP ${probe.status_code}` : ''}
                      {probe.latency_ms !== null ? ` - ${probe.latency_ms} ms` : ''}
                      {probe.error ? ` - ${probe.error}` : ''}
                    </dd>
                  </div>
                ))}
              </dl>
            )}
          </div>

          <div className="stack-sm">
            <h3 className="deploy-title">Running containers</h3>
            {result.containers.length === 0 ? (
              <p className="text-muted">No containers are running.</p>
            ) : (
              <dl className="data-grid">
                {result.containers.map((container) => (
                  <div key={container.id}>
                    <dt>{container.process_type}</dt>
                    <dd>
                      {container.status} - port {container.host_port}
                    </dd>
                  </div>
                ))}
              </dl>
            )}
          </div>
        </div>
      ) : running ? (
        <div className="loading-state">
          <Spinner />
          <span>Running checks…</span>
        </div>
      ) : null}
    </article>
  );
}
