import { type FormEvent, useCallback, useEffect, useMemo, useState } from 'react';
import type {
  App,
  EventRecord,
  LetsEncryptConfig,
  ManagedServiceSummary,
  ObjectStoreState,
  Plugin,
  RoutingStatusResponse,
  SshKey,
} from '../lib/api';
import {
  fetchApps,
  fetchEvents,
  fetchLetsEncryptConfig,
  fetchManagedServices,
  fetchObjectStoreConfig,
  fetchPlugins,
  fetchRoutingStatus,
  fetchSshKeys,
  getToken,
  setLetsEncryptConfig,
  testObjectStoreConfig,
} from '../lib/api';
import ConnectScreen from './ConnectScreen';

interface HostState {
  apps: App[];
  routing: RoutingStatusResponse;
  tls: LetsEncryptConfig;
  objectStore: ObjectStoreState;
  sshKeys: SshKey[];
  plugins: Plugin[];
  recentEvents: EventRecord[];
  services: {
    postgres: ManagedServiceSummary[];
    redis: ManagedServiceSummary[];
    mysql: ManagedServiceSummary[];
  };
}

export default function HostPage() {
  const [hasToken, setHasToken] = useState(() => Boolean(getToken()));

  if (!hasToken) {
    return <ConnectScreen onConnected={() => setHasToken(true)} />;
  }

  return <HostInner />;
}

function HostInner() {
  const [state, setState] = useState<HostState | null>(null);
  const [emailDraft, setEmailDraft] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const [
        apps,
        routing,
        tls,
        objectStore,
        sshKeys,
        plugins,
        recentEvents,
        postgres,
        redis,
        mysql,
      ] = await Promise.all([
        fetchApps(),
        fetchRoutingStatus(),
        fetchLetsEncryptConfig(),
        fetchObjectStoreConfig(),
        fetchSshKeys(),
        fetchPlugins(),
        fetchEvents(),
        fetchManagedServices('postgres'),
        fetchManagedServices('redis'),
        fetchManagedServices('mysql'),
      ]);

      const nextState = {
        apps,
        routing,
        tls,
        objectStore,
        sshKeys,
        plugins,
        recentEvents: recentEvents.slice(0, 18),
        services: {
          postgres,
          redis,
          mysql,
        },
      };

      setState(nextState);
      setEmailDraft(tls.email ?? '');
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to load host overview.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleSaveEmail(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const email = emailDraft.trim();
    if (!email) {
      setError('Email is required.');
      return;
    }

    try {
      setBusy('tls-email');
      setError(null);
      setNotice(null);
      await setLetsEncryptConfig(email);
      await load();
      setNotice(`Saved global Let's Encrypt email ${email}.`);
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to save Let’s Encrypt email.'
      );
    } finally {
      setBusy(null);
    }
  }

  async function handleTestObjectStore() {
    try {
      setBusy('objectstore-test');
      setError(null);
      setNotice(null);
      await testObjectStoreConfig();
      setNotice('Stored object store configuration passed the connectivity check.');
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to test object store config.'
      );
    } finally {
      setBusy(null);
    }
  }

  const metrics = useMemo(() => {
    if (!state) {
      return null;
    }

    const liveApps = state.apps.filter((app) => app.status === 'deployed').length;
    const lockedApps = state.apps.filter((app) => app.locked).length;
    const readyRoutes = state.routing.apps.filter((app) => app.status === 'ready').length;
    const totalServices =
      state.services.postgres.length + state.services.redis.length + state.services.mysql.length;

    return {
      totalApps: state.apps.length,
      liveApps,
      lockedApps,
      readyRoutes,
      totalServices,
    };
  }, [state]);

  if (loading) {
    return (
      <div className="panel loading-state">
        <span className="loading-spinner" />
        <span>Loading host overview…</span>
      </div>
    );
  }

  if (!state || !metrics) {
    return (
      <div className="panel error-state">
        Failed to load host overview: {error ?? 'unknown error'}
      </div>
    );
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <p className="eyebrow">Host admin</p>
          <h1 className="page-title">Platform overview</h1>
          <p className="page-copy">
            Track global routing, TLS, storage, services, access, and daemon-visible event flow in
            one place.
          </p>
        </div>
        <div className="metrics-grid">
          <Metric label="Apps" value={String(metrics.totalApps)} />
          <Metric label="Live apps" value={String(metrics.liveApps)} />
          <Metric label="Ready routes" value={String(metrics.readyRoutes)} />
          <Metric label="Services" value={String(metrics.totalServices)} />
        </div>
      </section>

      {notice ? <p className="callout callout-success">{notice}</p> : null}
      {error ? <p className="callout callout-danger">{error}</p> : null}

      <section className="panel-grid">
        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Fleet</p>
            <h2 className="section-title">App status</h2>
          </div>
          <dl className="data-grid">
            <div>
              <dt>Total apps</dt>
              <dd>{metrics.totalApps}</dd>
            </div>
            <div>
              <dt>Live apps</dt>
              <dd>{metrics.liveApps}</dd>
            </div>
            <div>
              <dt>Locked apps</dt>
              <dd>{metrics.lockedApps}</dd>
            </div>
            <div>
              <dt>TLS enabled</dt>
              <dd>{state.apps.filter((app) => app.tls_enabled).length}</dd>
            </div>
          </dl>
          <div className="button-row">
            <a className="btn btn-secondary btn-sm" href="/">
              Open apps
            </a>
          </div>
        </article>

        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Routing and TLS</p>
            <h2 className="section-title">Global edge status</h2>
          </div>
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

          <form onSubmit={handleSaveEmail} className="stack-md">
            <div className="form-group">
              <label className="form-label" htmlFor="host-le-email">
                Let’s Encrypt email
              </label>
              <input
                id="host-le-email"
                className="input"
                type="email"
                value={emailDraft}
                onChange={(event) => setEmailDraft(event.target.value)}
                placeholder="ops@example.com"
                disabled={busy !== null}
              />
            </div>
            <div className="form-actions">
              <button className="btn btn-primary" type="submit" disabled={busy !== null}>
                {busy === 'tls-email' ? 'Saving…' : 'Save email'}
              </button>
              <a className="btn btn-secondary" href="/routing">
                Open routing
              </a>
            </div>
          </form>
        </article>
      </section>

      <section className="panel-grid">
        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Durable storage</p>
            <h2 className="section-title">Object store</h2>
          </div>
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
          <div className="form-actions">
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => {
                void handleTestObjectStore();
              }}
              disabled={!state.objectStore.configured || busy !== null}
            >
              {busy === 'objectstore-test' ? 'Testing…' : 'Test config'}
            </button>
            <a className="btn btn-secondary" href="/object-store">
              Open object store
            </a>
          </div>
        </article>

        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Managed services</p>
            <h2 className="section-title">Datastores</h2>
          </div>
          <dl className="data-grid">
            <div>
              <dt>Postgres</dt>
              <dd>{state.services.postgres.length}</dd>
            </div>
            <div>
              <dt>Redis</dt>
              <dd>{state.services.redis.length}</dd>
            </div>
            <div>
              <dt>MySQL</dt>
              <dd>{state.services.mysql.length}</dd>
            </div>
            <div>
              <dt>Total</dt>
              <dd>{metrics.totalServices}</dd>
            </div>
          </dl>
          <div className="button-row">
            {state.services.postgres.slice(0, 2).map((service) => (
              <span key={service.name} className="service-state service-state-warning">
                {service.name}
              </span>
            ))}
          </div>
          <div className="form-actions">
            <a className="btn btn-secondary" href="/services">
              Open services
            </a>
          </div>
        </article>
      </section>

      <section className="panel-grid">
        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Access</p>
            <h2 className="section-title">SSH keys and plugins</h2>
          </div>
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
          <div className="stack-sm">
            <p className="eyebrow">Recent activity</p>
            <h2 className="section-title">Host event feed</h2>
          </div>
          {state.recentEvents.length === 0 ? (
            <p className="text-muted">No daemon events have been recorded yet.</p>
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <th>When</th>
                  <th>Type</th>
                  <th>App</th>
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
