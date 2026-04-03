import { type FormEvent, useCallback, useEffect, useState } from 'react';
import {
  type CertificateStatus,
  type PortMapping,
  type RoutingAppStatus,
  addPortMapping,
  disableTls,
  enableTls,
  fetchAppRoutingStatus,
  fetchPorts,
  fetchTlsStatus,
  removePortMapping,
} from '../lib/api';

interface AppRoutingPanelProps {
  appName: string;
  locked: boolean;
  onAppRefresh: () => Promise<void>;
}

interface RoutingState {
  ports: PortMapping[];
  routing: RoutingAppStatus;
  tls: CertificateStatus;
}

export default function AppRoutingPanel({ appName, locked, onAppRefresh }: AppRoutingPanelProps) {
  const [state, setState] = useState<RoutingState | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [hostPort, setHostPort] = useState('');
  const [containerPort, setContainerPort] = useState('');

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const [ports, routingStatus, tlsStatus] = await Promise.all([
        fetchPorts(appName),
        fetchAppRoutingStatus(appName),
        fetchTlsStatus(appName),
      ]);

      setState({
        ports,
        routing: routingStatus.app,
        tls: tlsStatus,
      });
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to load routing and TLS state.'
      );
    } finally {
      setLoading(false);
    }
  }, [appName]);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleAddPort(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextHostPort = Number(hostPort);
    const nextContainerPort = Number(containerPort);

    if (!Number.isFinite(nextHostPort) || nextHostPort <= 0) {
      setError('Host port must be a positive number.');
      return;
    }

    if (!Number.isFinite(nextContainerPort) || nextContainerPort <= 0) {
      setError('Container port must be a positive number.');
      return;
    }

    try {
      setBusy('port-add');
      setError(null);
      setNotice(null);
      await addPortMapping(appName, nextHostPort, nextContainerPort);
      setHostPort('');
      setContainerPort('');
      await load();
      setNotice(`Added port ${nextHostPort} -> ${nextContainerPort}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to add port mapping.');
    } finally {
      setBusy(null);
    }
  }

  async function handleRemovePort(port: PortMapping) {
    try {
      setBusy(`port-remove-${port.id}`);
      setError(null);
      setNotice(null);
      await removePortMapping(appName, port.id);
      await load();
      setNotice(`Removed port ${port.host_port} -> ${port.container_port}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to remove port mapping.');
    } finally {
      setBusy(null);
    }
  }

  async function handleToggleTls() {
    if (!state) return;

    try {
      setBusy('tls-toggle');
      setError(null);
      setNotice(null);

      if (state.tls.enabled) {
        await disableTls(appName);
      } else {
        await enableTls(appName);
      }

      await Promise.all([load(), onAppRefresh()]);
      setNotice(state.tls.enabled ? 'TLS disabled.' : 'TLS enabled.');
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to update TLS.');
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <span className="loading-spinner" />
        <span>Loading routing and TLS…</span>
      </div>
    );
  }

  if (!state) {
    return (
      <div className="panel error-state">
        Failed to load routing and TLS: {error ?? 'unknown error'}
      </div>
    );
  }

  return (
    <section className="panel-grid">
      <article className="panel panel-accent stack-md">
        <div className="cluster justify-between align-start">
          <div className="stack-sm">
            <p className="eyebrow">Routing inputs</p>
            <h2 className="section-title">Ports and upstreams</h2>
            <p className="page-copy">
              Add host-to-container port mappings that feed the Angie proxy configuration for this
              app.
            </p>
          </div>
          <span className="inventory-summary">
            {state.ports.length} upstream{state.ports.length === 1 ? '' : 's'}
          </span>
        </div>

        {notice ? <p className="callout callout-success">{notice}</p> : null}
        {error ? <p className="callout callout-danger">{error}</p> : null}

        <form onSubmit={handleAddPort} className="stack-md">
          <div className="panel-grid">
            <div className="form-group">
              <label className="form-label" htmlFor="host-port">
                Host port
              </label>
              <input
                id="host-port"
                className="input"
                type="number"
                min="1"
                value={hostPort}
                onChange={(event) => setHostPort(event.target.value)}
                disabled={locked || busy !== null}
                placeholder="8080"
              />
            </div>
            <div className="form-group">
              <label className="form-label" htmlFor="container-port">
                Container port
              </label>
              <input
                id="container-port"
                className="input"
                type="number"
                min="1"
                value={containerPort}
                onChange={(event) => setContainerPort(event.target.value)}
                disabled={locked || busy !== null}
                placeholder="3000"
              />
            </div>
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
              {busy === 'port-add' ? 'Adding…' : 'Add port'}
            </button>
          </div>
        </form>

        {state.ports.length === 0 ? (
          <p className="text-muted">
            No port mappings exist yet. Routing will remain incomplete until at least one upstream
            port is configured.
          </p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Host</th>
                <th>Container</th>
                <th>Protocol</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {state.ports.map((port) => (
                <tr key={port.id}>
                  <td className="font-mono">{port.host_port}</td>
                  <td className="font-mono">{port.container_port}</td>
                  <td className="font-mono">{port.protocol}</td>
                  <td>
                    <button
                      type="button"
                      className="btn btn-danger btn-sm"
                      disabled={locked || busy === `port-remove-${port.id}`}
                      onClick={() => handleRemovePort(port)}
                    >
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}

        <div className="stack-sm">
          <p className="eyebrow">Derived upstreams</p>
          <h3 className="deploy-title">Current proxy targets</h3>
        </div>
        {state.routing.upstreams.length === 0 ? (
          <p className="text-muted">No upstreams are currently derived for this app.</p>
        ) : (
          <dl className="connection-grid">
            {state.routing.upstreams.map((upstream, index) => (
              <div key={`${upstream.host}-${upstream.port}`} className="connection-card">
                <dt>Target {index + 1}</dt>
                <dd className="font-mono">
                  {upstream.host}:{upstream.port}
                </dd>
              </div>
            ))}
          </dl>
        )}
      </article>

      <article className="panel stack-md">
        <div className="cluster justify-between align-start">
          <div className="stack-sm">
            <p className="eyebrow">Proxy and TLS</p>
            <h2 className="section-title">Routing status</h2>
            <p className="page-copy">
              Inspect Angie readiness, certificate file state, and app-level TLS status for this
              route.
            </p>
          </div>
          <ServiceState status={state.routing.status} />
        </div>

        <dl className="data-grid">
          <div>
            <dt>TLS</dt>
            <dd>{state.tls.enabled ? 'Enabled' : 'Disabled'}</dd>
          </div>
          <div>
            <dt>Certificate</dt>
            <dd>{state.tls.ready ? 'Ready' : 'Missing files'}</dd>
          </div>
          <div>
            <dt>Proxy config</dt>
            <dd>{state.routing.proxy_config_present ? 'Present' : 'Missing'}</dd>
          </div>
          <div>
            <dt>Domains</dt>
            <dd className="font-mono">{state.routing.domains.length}</dd>
          </div>
        </dl>

        <div className="form-actions">
          <button
            type="button"
            className={state.tls.enabled ? 'btn btn-danger' : 'btn btn-secondary'}
            onClick={() => {
              void handleToggleTls();
            }}
            disabled={locked || busy === 'tls-toggle'}
          >
            {busy === 'tls-toggle' ? 'Saving…' : state.tls.enabled ? 'Disable TLS' : 'Enable TLS'}
          </button>
        </div>

        <div className="stack-sm">
          <p className="eyebrow">Certificate state</p>
          <h3 className="deploy-title">Files and inspection</h3>
        </div>
        <dl className="connection-grid">
          <div className="connection-card">
            <dt>Certificate path</dt>
            <dd className="font-mono">{state.tls.certificate.path}</dd>
          </div>
          <div className="connection-card">
            <dt>Private key path</dt>
            <dd className="font-mono">{state.tls.private_key.path}</dd>
          </div>
          <div className="connection-card">
            <dt>Not before</dt>
            <dd className="font-mono">{state.tls.not_before ?? 'Unavailable'}</dd>
          </div>
          <div className="connection-card">
            <dt>Not after</dt>
            <dd className="font-mono">{state.tls.not_after ?? 'Unavailable'}</dd>
          </div>
          <div className="connection-card">
            <dt>Subject</dt>
            <dd className="font-mono">{state.tls.subject ?? 'Unavailable'}</dd>
          </div>
          <div className="connection-card">
            <dt>Config path</dt>
            <dd className="font-mono">{state.routing.proxy_config_path}</dd>
          </div>
        </dl>

        {state.routing.issues.length === 0 && !state.tls.inspection_error ? (
          <p className="callout callout-success">
            Routing inputs and proxy configuration look healthy for this app.
          </p>
        ) : (
          <div className="stack-sm">
            <p className="eyebrow">Issues</p>
            <ul className="issue-list">
              {state.routing.issues.map((issue) => (
                <li key={issue} className="issue-item">
                  {issue}
                </li>
              ))}
              {state.tls.inspection_error ? (
                <li className="issue-item">{state.tls.inspection_error}</li>
              ) : null}
            </ul>
          </div>
        )}
      </article>
    </section>
  );
}

function ServiceState({ status }: { status: string }) {
  const normalized = status.toLowerCase();
  const tone =
    normalized.includes('ready') || normalized.includes('ok')
      ? 'success'
      : normalized.includes('degraded') || normalized.includes('missing')
        ? 'danger'
        : 'warning';

  return <span className={`service-state service-state-${tone}`}>{status}</span>;
}
