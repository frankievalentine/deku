import { useMutation, useQueryClient } from '@tanstack/react-query';
import { type SubmitEvent, useMemo, useState } from 'react';
import {
  addPortMapping,
  type CertificateStatus,
  disableTls,
  enableTls,
  type PortMapping,
  type RoutingAppStatus,
  removePortMapping,
} from '../lib/api';
import {
  getErrorMessage,
  getFirstQueryError,
  invalidateAppRoutingQueries,
  useAppPortsQuery,
  useAppRoutingStatusQuery,
  useAppTlsStatusQuery,
} from '../lib/query';
import TableScroll from './TableScroll';

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
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [hostPort, setHostPort] = useState('');
  const [containerPort, setContainerPort] = useState('');
  const portsQuery = useAppPortsQuery(appName, { enabled: Boolean(appName) });
  const routingQuery = useAppRoutingStatusQuery(appName, { enabled: Boolean(appName) });
  const tlsQuery = useAppTlsStatusQuery(appName, { enabled: Boolean(appName) });
  const addPortMutation = useMutation({
    mutationFn: ({
      nextHostPort,
      nextContainerPort,
    }: {
      nextHostPort: number;
      nextContainerPort: number;
    }) => addPortMapping(appName, nextHostPort, nextContainerPort),
    onSuccess: async () => {
      await invalidateAppRoutingQueries(queryClient, appName);
    },
  });
  const removePortMutation = useMutation({
    mutationFn: (portId: string) => removePortMapping(appName, portId),
    onSuccess: async () => {
      await invalidateAppRoutingQueries(queryClient, appName);
    },
  });
  const toggleTlsMutation = useMutation({
    mutationFn: (enabled: boolean) => (enabled ? disableTls(appName) : enableTls(appName)),
    onSuccess: async () => {
      await Promise.all([invalidateAppRoutingQueries(queryClient, appName), onAppRefresh()]);
    },
  });

  const state = useMemo<RoutingState | null>(() => {
    if (!portsQuery.data || !routingQuery.data || !tlsQuery.data) {
      return null;
    }

    return {
      ports: portsQuery.data,
      routing: routingQuery.data.app,
      tls: tlsQuery.data,
    };
  }, [portsQuery.data, routingQuery.data, tlsQuery.data]);

  const loading = !state && [portsQuery, routingQuery, tlsQuery].some((query) => query.isPending);
  const queryError = getFirstQueryError(
    [portsQuery.error, routingQuery.error, tlsQuery.error],
    state ? null : 'Unable to load routing and TLS state.'
  );
  const error = actionError ?? queryError;

  async function handleAddPort(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextHostPort = Number(hostPort);
    const nextContainerPort = Number(containerPort);

    if (!Number.isFinite(nextHostPort) || nextHostPort <= 0) {
      setActionError('Host port must be a positive number.');
      return;
    }

    if (!Number.isFinite(nextContainerPort) || nextContainerPort <= 0) {
      setActionError('Container port must be a positive number.');
      return;
    }

    try {
      setBusy('port-add');
      setActionError(null);
      setNotice(null);
      await addPortMutation.mutateAsync({ nextHostPort, nextContainerPort });
      setHostPort('');
      setContainerPort('');
      setNotice(`Added port ${nextHostPort} -> ${nextContainerPort}.`);
    } catch (nextError) {
      setActionError(getErrorMessage(nextError, 'Unable to add port mapping.'));
    } finally {
      setBusy(null);
    }
  }

  async function handleRemovePort(port: PortMapping) {
    try {
      setBusy(`port-remove-${port.id}`);
      setActionError(null);
      setNotice(null);
      await removePortMutation.mutateAsync(port.id);
      setNotice(`Removed port ${port.host_port} -> ${port.container_port}.`);
    } catch (nextError) {
      setActionError(getErrorMessage(nextError, 'Unable to remove port mapping.'));
    } finally {
      setBusy(null);
    }
  }

  async function handleToggleTls() {
    if (!state) return;

    try {
      setBusy('tls-toggle');
      setActionError(null);
      setNotice(null);
      await toggleTlsMutation.mutateAsync(state.tls.enabled);
      setNotice(state.tls.enabled ? 'TLS disabled.' : 'TLS enabled.');
    } catch (nextError) {
      setActionError(getErrorMessage(nextError, 'Unable to update TLS.'));
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return <RoutingPanelSkeleton />;
  }

  if (!state) {
    return (
      <div className="panel error-state">
        Failed to load routing and TLS: {error ?? 'unknown error'}
      </div>
    );
  }

  return (
    <section className="panel-grid app-routing-grid">
      <article className="panel panel-accent stack-md routing-input-card">
        <div className="panel-heading">
          <div className="stack-sm panel-heading-copy">
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
          <TableScroll>
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
          </TableScroll>
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
        <div className="panel-heading">
          <div className="stack-sm panel-heading-copy">
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

function RoutingPanelSkeleton() {
  return (
    <section className="panel-grid app-routing-grid">
      <article className="panel panel-accent stack-md routing-input-card">
        <div className="panel-heading">
          <div className="stack-sm panel-heading-copy">
            <SkeletonBlock className="h-3 w-28" />
            <SkeletonBlock className="h-7 w-52" />
            <SkeletonBlock className="h-4 w-full max-w-md" />
            <SkeletonBlock className="h-4 w-4/5 max-w-sm" />
          </div>
          <SkeletonBlock className="h-5 w-24" />
        </div>

        <div className="panel-grid">
          <div className="stack-md">
            <div className="form-group">
              <SkeletonBlock className="h-3 w-20" />
              <SkeletonBlock className="h-11 w-full" />
            </div>
          </div>
          <div className="stack-md">
            <div className="form-group">
              <SkeletonBlock className="h-3 w-24" />
              <SkeletonBlock className="h-11 w-full" />
            </div>
          </div>
        </div>

        <div className="form-actions">
          <SkeletonBlock className="h-10 w-28" />
        </div>

        <div className="stack-sm">
          <SkeletonBlock className="h-3 w-28" />
          <SkeletonBlock className="h-6 w-40" />
        </div>

        <div className="connection-grid">
          {skeletonItems('upstream', 2).map((item) => (
            <div key={item} className="connection-card stack-sm">
              <SkeletonBlock className="h-3 w-20" />
              <SkeletonBlock className="h-4 w-full" />
            </div>
          ))}
        </div>
      </article>

      <article className="panel stack-md">
        <div className="panel-heading">
          <div className="stack-sm panel-heading-copy">
            <SkeletonBlock className="h-3 w-24" />
            <SkeletonBlock className="h-7 w-44" />
            <SkeletonBlock className="h-4 w-full max-w-md" />
            <SkeletonBlock className="h-4 w-3/4 max-w-sm" />
          </div>
          <SkeletonBlock className="h-6 w-24 rounded-full" />
        </div>

        <div className="data-grid">
          {skeletonItems('status', 4).map((item) => (
            <div key={item} className="stack-sm">
              <SkeletonBlock className="h-3 w-20" />
              <SkeletonBlock className="h-4 w-24" />
            </div>
          ))}
        </div>

        <div className="form-actions">
          <SkeletonBlock className="h-10 w-32" />
        </div>

        <div className="stack-sm">
          <SkeletonBlock className="h-3 w-28" />
          <SkeletonBlock className="h-6 w-40" />
        </div>

        <div className="connection-grid">
          {skeletonItems('certificate', 4).map((item) => (
            <div key={item} className="connection-card stack-sm">
              <SkeletonBlock className="h-3 w-24" />
              <SkeletonBlock className="h-4 w-full" />
            </div>
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
