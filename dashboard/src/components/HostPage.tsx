import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import type {
  AlertRecord,
  App,
  EventRecord,
  LetsEncryptConfig,
  ManagedServiceKind,
  ManagedServiceSummary,
  ObjectStoreState,
  Plugin,
  RoutingStatusResponse,
  SshKey,
} from '../lib/api';
import {
  fetchAlerts,
  fetchApps,
  fetchEvents,
  fetchLetsEncryptConfig,
  fetchManagedServices,
  fetchObjectStoreConfig,
  fetchPlugins,
  fetchRoutingStatus,
  fetchSshKeys,
  MANAGED_SERVICE_KINDS,
  testObjectStoreConfig,
} from '../lib/api';
import { SERVICE_KIND_LABELS } from '../lib/service-meta';
import { showToast } from '../lib/shell';
import ConnectScreen from './ConnectScreen';
import ServiceStateBadge from './ServiceStateBadge';
import Spinner from './Spinner';
import TableScroll from './TableScroll';

interface HostState {
  apps: App[];
  routing: RoutingStatusResponse;
  tls: LetsEncryptConfig;
  objectStore: ObjectStoreState;
  sshKeys: SshKey[];
  plugins: Plugin[];
  recentEvents: EventRecord[];
  alerts: AlertRecord[];
  services: Record<ManagedServiceKind, ManagedServiceSummary[]>;
}
const EMPTY_HOST_STATE: HostState = {
  apps: [],
  routing: {
    angie: {
      config_valid: false,
      validation_error: null,
    },
    apps: [],
  },
  tls: {
    configured: false,
    email: null,
  },
  objectStore: {
    configured: false,
    object_store: null,
  },
  sshKeys: [],
  plugins: [],
  recentEvents: [],
  alerts: [],
  services: emptyServiceKindRecord(),
};

function emptyServiceKindRecord(): Record<ManagedServiceKind, ManagedServiceSummary[]> {
  return {
    postgres: [],
    redis: [],
    mysql: [],
    mariadb: [],
    mongodb: [],
  };
}

export default function HostPage() {
  const tokenAccess = useTokenAccess();

  if (tokenAccess === 'unknown') {
    return null;
  }

  if (tokenAccess === 'locked') {
    return <ConnectScreen />;
  }

  return <HostInner />;
}

function HostInner() {
  const [state, setState] = useState<HostState>(EMPTY_HOST_STATE);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setLoadError(null);
      const [
        apps,
        routing,
        tls,
        objectStore,
        sshKeys,
        plugins,
        recentEvents,
        alerts,
        ...serviceLists
      ] = await Promise.all([
        fetchApps(),
        fetchRoutingStatus(),
        fetchLetsEncryptConfig(),
        fetchObjectStoreConfig(),
        fetchSshKeys(),
        fetchPlugins(),
        fetchEvents(),
        fetchAlerts(),
        ...MANAGED_SERVICE_KINDS.map((kind) => fetchManagedServices(kind)),
      ]);

      const nextState = {
        apps,
        routing,
        tls,
        objectStore,
        sshKeys,
        plugins,
        recentEvents: recentEvents.slice(0, 18),
        alerts,
        services: Object.fromEntries(
          MANAGED_SERVICE_KINDS.map((kind, index) => [kind, serviceLists[index]])
        ) as Record<ManagedServiceKind, ManagedServiceSummary[]>,
      };

      setState(nextState);
    } catch (nextError) {
      setLoadError(
        nextError instanceof Error ? nextError.message : 'Unable to load host overview.'
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleTestObjectStore() {
    try {
      setBusy('objectstore-test');
      setActionError(null);
      await testObjectStoreConfig();
      showToast({
        title: 'Object store test passed',
        description: 'Stored object store configuration passed the connectivity check.',
        variant: 'success',
      });
    } catch (nextError) {
      setActionError(
        nextError instanceof Error ? nextError.message : 'Unable to test object store config.'
      );
    } finally {
      setBusy(null);
    }
  }

  const metrics = useMemo(() => {
    const liveApps = state.apps.filter((app) => app.status === 'deployed').length;
    const lockedApps = state.apps.filter((app) => app.locked).length;
    const readyRoutes = state.routing.apps.filter((app) => app.status === 'ready').length;
    const totalServices = Object.values(state.services).reduce(
      (count, list) => count + list.length,
      0
    );

    return {
      totalApps: state.apps.length,
      liveApps,
      lockedApps,
      readyRoutes,
      totalServices,
    };
  }, [state]);

  // Flattened for the datastore badges, which span every service kind.
  const allServices = useMemo(
    () => MANAGED_SERVICE_KINDS.flatMap((kind) => state.services[kind]),
    [state.services]
  );

  if (loading) {
    return (
      <div className="panel loading-state">
        <Spinner />
        <span>Loading host overview…</span>
      </div>
    );
  }

  if (loadError) {
    return (
      <div className="panel error-state">
        <p className="text-danger" role="alert">
          {loadError}
        </p>
        <button type="button" className="btn btn-secondary" onClick={() => void load()}>
          Retry loading host overview
        </button>
      </div>
    );
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <h1 className="page-title">Platform overview</h1>
          <p className="page-copy">Routing, TLS, storage, services, and access for this host.</p>
        </div>
        <div className="metrics-grid">
          <Metric label="Apps" value={String(metrics.totalApps)} />
          <Metric label="Live apps" value={String(metrics.liveApps)} />
          <Metric label="Ready routes" value={String(metrics.readyRoutes)} />
          <Metric label="Services" value={String(metrics.totalServices)} />
        </div>
      </section>

      {actionError ? (
        <p className="callout callout-danger" role="alert">
          {actionError}
        </p>
      ) : null}

      <section className="panel-grid">
        <article className="panel summary-card">
          <div className="summary-card-body">
            <h2 className="section-title">App status</h2>
            <dl className="data-grid">
              <div>
                <dt>Locked apps</dt>
                <dd>{metrics.lockedApps}</dd>
              </div>
              <div>
                <dt>TLS enabled</dt>
                <dd>{state.apps.filter((app) => app.tls_enabled).length}</dd>
              </div>
            </dl>
          </div>
          <div className="form-actions summary-card-actions">
            <a className="btn btn-secondary" href="/">
              Open apps
            </a>
          </div>
        </article>

        <article className="panel summary-card">
          <div className="summary-card-body">
            <h2 className="section-title">Global edge status</h2>
            <dl className="data-grid">
              <div>
                <dt>Angie validation</dt>
                <dd>{state.routing.angie.config_valid ? 'Valid' : 'Invalid'}</dd>
              </div>
              <div>
                <dt>Ready routes</dt>
                <dd>{metrics.readyRoutes}</dd>
              </div>
              <div>
                <dt>TLS email</dt>
                <dd className="font-mono">{state.tls.email ?? 'Unset'}</dd>
              </div>
              <div>
                <dt>Apps with issues</dt>
                <dd>{state.routing.apps.filter((app) => app.issues.length > 0).length}</dd>
              </div>
            </dl>
          </div>
          <div className="form-actions summary-card-actions">
            <a className="btn btn-secondary" href="/routing">
              Open routing
            </a>
            <a className="btn btn-primary" href="/settings">
              Open settings
            </a>
          </div>
        </article>
      </section>

      <section className="panel-grid">
        <article className="panel summary-card">
          <div className="summary-card-body">
            <h2 className="section-title">Object store</h2>
            <dl className="data-grid">
              <div>
                <dt>Configured</dt>
                <dd>{state.objectStore.configured ? 'Yes' : 'No'}</dd>
              </div>
              <div>
                <dt>Provider</dt>
                <dd>{state.objectStore.object_store?.provider ?? 'Unset'}</dd>
              </div>
              <div>
                <dt>Bucket</dt>
                <dd className="font-mono">{state.objectStore.object_store?.bucket ?? 'Unset'}</dd>
              </div>
              <div>
                <dt>Endpoint</dt>
                <dd className="font-mono">{state.objectStore.object_store?.endpoint ?? 'Unset'}</dd>
              </div>
            </dl>
            {state.objectStore.configured ? null : (
              <p id="host-object-store-test-hint" className="text-muted">
                Save an object store configuration to enable the connectivity test.
              </p>
            )}
          </div>
          <div className="form-actions summary-card-actions">
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => {
                void handleTestObjectStore();
              }}
              disabled={!state.objectStore.configured || busy !== null}
              aria-describedby={
                state.objectStore.configured ? undefined : 'host-object-store-test-hint'
              }
            >
              {busy === 'objectstore-test' ? 'Testing…' : 'Test config'}
            </button>
            <a className="btn btn-secondary" href="/object-store">
              Open object store
            </a>
          </div>
        </article>

        <article className="panel summary-card">
          <div className="summary-card-body">
            <h2 className="section-title">Datastores</h2>
            <dl className="data-grid">
              {MANAGED_SERVICE_KINDS.map((kind) => (
                <div key={kind}>
                  <dt>{SERVICE_KIND_LABELS[kind]}</dt>
                  <dd>{state.services[kind].length}</dd>
                </div>
              ))}
              <div>
                <dt>Total</dt>
                <dd>{metrics.totalServices}</dd>
              </div>
            </dl>
            {metrics.totalServices === 0 ? (
              <p className="text-muted">
                No managed services yet. Open services to provision Postgres, MySQL, MariaDB, Redis,
                or MongoDB.
              </p>
            ) : (
              <div className="button-row">
                {allServices.slice(0, 2).map((service) => (
                  <ServiceStateBadge
                    key={`${service.plugin}:${service.name}`}
                    label={service.name}
                    status={service.status}
                  />
                ))}
                {allServices.length > 2 ? (
                  <span className="text-muted">and {allServices.length - 2} more</span>
                ) : null}
              </div>
            )}
          </div>
          <div className="form-actions summary-card-actions">
            <a className="btn btn-secondary" href="/services">
              Open services
            </a>
          </div>
        </article>
      </section>

      <section className="panel-grid">
        <article className="panel stack-md">
          <h2 className="section-title">SSH keys and plugins</h2>
          <dl className="data-grid">
            <div>
              <dt>SSH keys</dt>
              <dd>{state.sshKeys.length}</dd>
            </div>
            <div>
              <dt>Plugins</dt>
              <dd>{state.plugins.length}</dd>
            </div>
          </dl>
          <div className="form-actions">
            <a className="btn btn-secondary" href="/ssh-keys">
              Open SSH keys
            </a>
            <a className="btn btn-secondary" href="/plugins">
              Open plugins
            </a>
          </div>
        </article>

        <article className="panel stack-md">
          <h2 className="section-title">Alerts</h2>
          {state.alerts.length === 0 ? (
            <p className="text-muted">
              No active alerts. Certificates, container health, backups, object store reachability,
              and disk usage are checked on a timer.
            </p>
          ) : (
            <TableScroll>
              <table className="table">
                <caption className="sr-only">Active alerts for this host</caption>
                <thead>
                  <tr>
                    <th scope="col">Severity</th>
                    <th scope="col">Rule</th>
                    <th scope="col">Subject</th>
                    <th scope="col">Since</th>
                    <th scope="col">Message</th>
                  </tr>
                </thead>
                <tbody>
                  {state.alerts.map((alert) => (
                    <tr key={alert.id}>
                      <td className="font-mono">{alert.severity}</td>
                      <td className="font-mono">{alert.rule}</td>
                      <td className="font-mono">{alert.subject}</td>
                      <td className="font-mono">{formatDate(alert.first_seen_at)}</td>
                      <td>{alert.message}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </TableScroll>
          )}
        </article>

        <article className="panel stack-md">
          <h2 className="section-title">Host event feed</h2>
          {state.recentEvents.length === 0 ? (
            <p className="text-muted">
              No daemon events yet. Deploys, service changes, and token rotations appear here.
            </p>
          ) : (
            <TableScroll>
              <table className="table">
                <caption className="sr-only">Recent daemon events for this host</caption>
                <thead>
                  <tr>
                    <th scope="col">When</th>
                    <th scope="col">Type</th>
                    <th scope="col">App</th>
                  </tr>
                </thead>
                <tbody>
                  {state.recentEvents.map((event) => (
                    <tr key={event.id}>
                      <td className="font-mono">{formatDate(event.created_at)}</td>
                      <td className="font-mono">{event.event_type}</td>
                      <td className="font-mono">
                        {event.app_id ? event.app_id.slice(0, 8) : 'host'}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </TableScroll>
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

function formatDate(value: string): string {
  return new Date(value).toLocaleString('en-US', {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}
