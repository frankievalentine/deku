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
  fetchRoutingStatus,
  fetchSshKeys,
  MANAGED_SERVICE_KINDS,
} from '../lib/api';
import ConnectScreen from './ConnectScreen';
import HostNetworksPanel from './HostNetworksPanel';
import HostStatusBand from './HostStatusBand';
import Spinner from './Spinner';
import TableScroll from './TableScroll';

interface HostState {
  apps: App[];
  routing: RoutingStatusResponse;
  tls: LetsEncryptConfig;
  objectStore: ObjectStoreState;
  sshKeys: SshKey[];
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

type HealthTone = 'success' | 'warning' | 'danger';

interface HealthCheck {
  label: string;
  status: string;
  tone: HealthTone;
  href: string;
}

function HostInner() {
  const [state, setState] = useState<HostState>(EMPTY_HOST_STATE);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setLoadError(null);
      const [apps, routing, tls, objectStore, sshKeys, recentEvents, alerts, ...serviceLists] =
        await Promise.all([
          fetchApps(),
          fetchRoutingStatus(),
          fetchLetsEncryptConfig(),
          fetchObjectStoreConfig(),
          fetchSshKeys(),
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

  const metrics = useMemo(() => {
    const liveApps = state.apps.filter((app) => app.status === 'deployed').length;
    const readyRoutes = state.routing.apps.filter((app) => app.status === 'ready').length;
    const totalServices = Object.values(state.services).reduce(
      (count, list) => count + list.length,
      0
    );

    return {
      totalApps: state.apps.length,
      liveApps,
      readyRoutes,
      totalServices,
    };
  }, [state]);

  const checks = useMemo<HealthCheck[]>(() => {
    const appsWithIssues = state.routing.apps.filter((app) => app.issues.length > 0).length;

    return [
      {
        label: 'Proxy configuration',
        status: state.routing.angie.config_valid ? 'Valid' : 'Invalid',
        tone: state.routing.angie.config_valid ? 'success' : 'danger',
        href: '/routing',
      },
      {
        label: 'Certificate email',
        status: state.tls.configured ? 'Configured' : 'Not set',
        tone: state.tls.configured ? 'success' : 'warning',
        href: '/routing',
      },
      {
        label: 'Object store',
        status: state.objectStore.configured ? 'Configured' : 'Not configured',
        tone: state.objectStore.configured ? 'success' : 'warning',
        href: '/storage',
      },
      {
        label: 'Apps with routing issues',
        status: String(appsWithIssues),
        tone: appsWithIssues === 0 ? 'success' : 'warning',
        href: '/routing',
      },
      {
        label: 'SSH keys',
        status: String(state.sshKeys.length),
        tone: 'success',
        href: '/access',
      },
    ];
  }, [state]);

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
      <HostStatusBand
        inputs={{
          totalApps: metrics.totalApps,
          liveApps: metrics.liveApps,
          readyRoutes: metrics.readyRoutes,
          totalServices: metrics.totalServices,
          alertCount: state.alerts.length,
        }}
      />

      <section className="panel-grid">
        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Health</p>
            <h2 className="section-title">Platform checks</h2>
          </div>

          <TableScroll>
            <table className="table">
              <caption className="sr-only">Host health checks</caption>
              <thead>
                <tr>
                  <th scope="col">Check</th>
                  <th scope="col">Status</th>
                  <th scope="col">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {checks.map((check) => (
                  <tr key={check.label}>
                    <td>{check.label}</td>
                    <td>
                      <span className={`service-state service-state-${check.tone}`}>
                        {check.status}
                      </span>
                    </td>
                    <td>
                      <a className="btn btn-secondary btn-sm" href={check.href}>
                        Open
                        <span className="sr-only"> {check.label}</span>
                      </a>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableScroll>
        </article>

        <article className="panel stack-md">
          <h2 className="section-title">Alerts</h2>
          {state.alerts.length === 0 ? (
            <p className="text-muted">
              No active alerts. Certificates, container health, backups, object store reachability,
              and disk usage are checked on a timer.
            </p>
          ) : (
            <ul className="alert-list">
              {state.alerts.map((alert) => (
                <li key={alert.id} className={`alert-row alert-row-${alert.severity}`}>
                  <div className="cluster alert-row-head">
                    <span
                      className={`service-state service-state-${alert.severity === 'critical' ? 'danger' : 'warning'}`}
                    >
                      {alert.severity}
                    </span>
                    <span className="alert-row-message">{alert.message}</span>
                  </div>
                  <div className="cluster alert-row-meta">
                    <span className="font-mono">{alert.rule}</span>
                    <span className="font-mono">{alert.subject}</span>
                    <span className="font-mono">{formatDate(alert.first_seen_at)}</span>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </article>
      </section>

      <HostNetworksPanel />

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
