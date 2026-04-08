import { useCallback, useEffect, useState } from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import type { LetsEncryptConfig, RoutingStatusResponse, RoutingTableEntry } from '../lib/api';
import { fetchLetsEncryptConfig, fetchRoutingStatus, fetchRoutingTable } from '../lib/api';
import ConnectScreen from './ConnectScreen';
import TableScroll from './TableScroll';

const EMPTY_ROUTING_STATUS: RoutingStatusResponse = {
  angie: {
    config_valid: false,
    validation_error: null,
  },
  apps: [],
};
const EMPTY_TLS_CONFIG: LetsEncryptConfig = {
  configured: false,
  email: null,
};

export default function RoutingPage() {
  const tokenAccess = useTokenAccess();

  if (tokenAccess === 'unknown') {
    return null;
  }

  if (tokenAccess === 'locked') {
    return <ConnectScreen />;
  }

  return <RoutingInner />;
}

function RoutingInner() {
  const [table, setTable] = useState<RoutingTableEntry[]>([]);
  const [status, setStatus] = useState<RoutingStatusResponse>(EMPTY_ROUTING_STATUS);
  const [tlsConfig, setTlsConfig] = useState<LetsEncryptConfig>(EMPTY_TLS_CONFIG);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setError(null);
      const [nextTable, nextStatus, nextTlsConfig] = await Promise.all([
        fetchRoutingTable(),
        fetchRoutingStatus(),
        fetchLetsEncryptConfig(),
      ]);

      setTable(nextTable);
      setStatus(nextStatus);
      setTlsConfig(nextTlsConfig);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to load routing overview.');
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <p className="eyebrow">Routing and TLS</p>
          <h1 className="page-title">Host routing overview</h1>
          <p className="page-copy">
            Inspect Angie validation, routing table coverage, and global Let’s Encrypt configuration
            across all apps.
          </p>
        </div>
        <div className="metrics-grid">
          <Metric label="Apps" value={String(status.apps.length)} />
          <Metric label="Angie" value={status.angie.config_valid ? 'Valid' : 'Invalid'} />
          <Metric label="TLS Email" value={tlsConfig.configured ? 'Configured' : 'Missing'} />
          <Metric
            label="Ready Routes"
            value={String(status.apps.filter((app) => app.status === 'ready').length)}
          />
        </div>
      </section>

      {error ? <p className="callout callout-danger">{error}</p> : null}

      <section className="panel-grid">
        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Angie</p>
            <h2 className="section-title">Proxy validation</h2>
          </div>
          <p
            className={`callout ${status.angie.config_valid ? 'callout-success' : 'callout-danger'}`}
          >
            {status.angie.config_valid
              ? 'Angie configuration validates successfully.'
              : (status.angie.validation_error ?? 'Validation failed.')}
          </p>
        </article>

        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Global TLS</p>
            <h2 className="section-title">Let’s Encrypt email</h2>
          </div>
          <p className="text-muted">
            {tlsConfig.configured
              ? `Current email: ${tlsConfig.email}`
              : 'No global Let’s Encrypt email has been configured yet.'}
          </p>
          <p className="page-copy">
            Update the account email from Settings. Routing now links to that shared configuration
            instead of editing it inline.
          </p>
          <div className="form-actions">
            <a className="btn btn-primary" href="/settings">
              Open settings
            </a>
          </div>
        </article>
      </section>

      <article className="panel stack-md">
        <div className="stack-sm">
          <p className="eyebrow">Routing table</p>
          <h2 className="section-title">App to upstream map</h2>
        </div>
        {table.length === 0 ? (
          <p className="text-muted">No routing entries are currently published.</p>
        ) : (
          <TableScroll>
            <table className="table">
              <thead>
                <tr>
                  <th>App</th>
                  <th>Domains</th>
                  <th>Upstreams</th>
                </tr>
              </thead>
              <tbody>
                {table.map((entry) => (
                  <tr key={entry.app}>
                    <td>{entry.app}</td>
                    <td className="font-mono">
                      {entry.domains.length === 0 ? 'none' : entry.domains.join(', ')}
                    </td>
                    <td className="font-mono">
                      {entry.upstreams.length === 0
                        ? 'none'
                        : entry.upstreams
                            .map((upstream) => `${upstream.host}:${upstream.port}`)
                            .join(', ')}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableScroll>
        )}
      </article>

      <article className="panel stack-md">
        <div className="stack-sm">
          <p className="eyebrow">Per-app status</p>
          <h2 className="section-title">Routing health</h2>
        </div>
        {status.apps.length === 0 ? (
          <p className="text-muted">No apps are available yet.</p>
        ) : (
          <TableScroll>
            <table className="table">
              <thead>
                <tr>
                  <th>App</th>
                  <th>Status</th>
                  <th>TLS</th>
                  <th>Proxy config</th>
                  <th>Issues</th>
                </tr>
              </thead>
              <tbody>
                {status.apps.map((app) => (
                  <tr key={app.app}>
                    <td>
                      <a href={`/app?name=${encodeURIComponent(app.app)}`}>{app.app}</a>
                    </td>
                    <td>
                      <ServiceState status={app.status} />
                    </td>
                    <td>{app.tls_enabled ? (app.tls_ready ? 'Ready' : 'Enabled') : 'Off'}</td>
                    <td className="font-mono">
                      {app.proxy_config_present ? 'Present' : 'Missing'}
                    </td>
                    <td>{app.issues.length === 0 ? 'None' : app.issues.join(' | ')}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableScroll>
        )}
      </article>
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
    normalized.includes('ready') || normalized.includes('valid')
      ? 'success'
      : normalized.includes('degraded') || normalized.includes('invalid')
        ? 'danger'
        : 'warning';

  return <span className={`service-state service-state-${tone}`}>{status}</span>;
}
