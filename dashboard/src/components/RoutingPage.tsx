import { useTokenAccess } from '../hooks/useHasToken';
import type { LetsEncryptConfig, RoutingStatusResponse, RoutingTableEntry } from '../lib/api';
import {
  getFirstQueryError,
  useLetsEncryptConfigQuery,
  useRoutingStatusQuery,
  useRoutingTableQuery,
} from '../lib/query';
import ConnectScreen from './ConnectScreen';
import TableScroll from './TableScroll';
import Spinner from './Spinner';

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
  const tableQuery = useRoutingTableQuery({ refetchInterval: 30_000 });
  const statusQuery = useRoutingStatusQuery({ refetchInterval: 30_000 });
  const tlsConfigQuery = useLetsEncryptConfigQuery();
  const table = tableQuery.data ?? EMPTY_ROUTING_TABLE;
  const status = statusQuery.data ?? EMPTY_ROUTING_STATUS;
  const tlsConfig = tlsConfigQuery.data ?? EMPTY_TLS_CONFIG;
  const error = getFirstQueryError(
    [tableQuery.error, statusQuery.error, tlsConfigQuery.error],
    null
  );
  const loading = tableQuery.isPending || statusQuery.isPending;
  const loaded = tableQuery.data !== undefined && statusQuery.data !== undefined;

  function retryAll() {
    void tableQuery.refetch();
    void statusQuery.refetch();
    void tlsConfigQuery.refetch();
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <Spinner />
        <span>Loading routing status…</span>
      </div>
    );
  }

  if (error && !loaded) {
    return (
      <div className="panel error-state">
        <p className="text-danger" role="alert">
          {error}
        </p>
        <button type="button" className="btn btn-secondary" onClick={retryAll}>
          Retry loading routing status
        </button>
      </div>
    );
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <h1 className="page-title">Routing</h1>
          <p className="page-copy">
            Published routes, proxy validation, and Let’s Encrypt configuration for every app.
          </p>
        </div>
        <div className="metrics-grid">
          <Metric label="Apps" value={String(status.apps.length)} />
          <Metric label="Proxy config" value={status.angie.config_valid ? 'Valid' : 'Invalid'} />
          <Metric label="TLS email" value={tlsConfig.configured ? 'Configured' : 'Missing'} />
          <Metric
            label="Ready routes"
            value={String(status.apps.filter((app) => app.status === 'ready').length)}
          />
        </div>
      </section>

      {error ? (
        <p className="callout callout-danger" role="alert">
          {error}
        </p>
      ) : null}

      <section className="panel-grid">
        <article className="panel stack-md">
          <h2 className="section-title">Proxy configuration</h2>
          <p
            className={`callout ${status.angie.config_valid ? 'callout-success' : 'callout-danger'}`}
          >
            {status.angie.config_valid
              ? 'Angie configuration is valid.'
              : (status.angie.validation_error ??
                'Angie configuration is invalid. Fix the reported config error and reload the proxy.')}
          </p>
        </article>

        <article className="panel stack-md">
          <h2 className="section-title">Let’s Encrypt email</h2>
          <p className="text-muted">
            {tlsConfig.configured
              ? `Current email: ${tlsConfig.email}`
              : 'No account email is set, so certificate operations cannot run.'}
          </p>
          <div className="form-actions">
            <a className="btn btn-primary" href="/settings">
              Update Let’s Encrypt email
            </a>
          </div>
        </article>
      </section>

      <article className="panel stack-md">
        <h2 className="section-title">App to upstream map</h2>
        {table.length === 0 ? (
          <p className="text-muted">
            No routes are published yet. Add a domain to an app and deploy it to publish one.
          </p>
        ) : (
          <TableScroll>
            <table className="table">
              <caption className="sr-only">
                Routing table mapping each app to its domains and upstreams
              </caption>
              <thead>
                <tr>
                  <th scope="col">App</th>
                  <th scope="col">Domains</th>
                  <th scope="col">Upstreams</th>
                </tr>
              </thead>
              <tbody>
                {table.map((entry) => (
                  <tr key={entry.app}>
                    <td>{entry.app}</td>
                    <td className="font-mono">
                      {entry.domains.length === 0 ? 'None' : entry.domains.join(', ')}
                    </td>
                    <td className="font-mono">
                      {entry.upstreams.length === 0
                        ? 'None'
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
        <h2 className="section-title">Routing health</h2>
        {status.apps.length === 0 ? (
          <p className="text-muted">
            No apps are available. Create an app to see its routing status here.
          </p>
        ) : (
          <TableScroll>
            <table className="table">
              <caption className="sr-only">Routing status for each app</caption>
              <thead>
                <tr>
                  <th scope="col">App</th>
                  <th scope="col">Status</th>
                  <th scope="col">TLS</th>
                  <th scope="col">Proxy config</th>
                  <th scope="col">Issues</th>
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

const EMPTY_ROUTING_TABLE: RoutingTableEntry[] = [];

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
