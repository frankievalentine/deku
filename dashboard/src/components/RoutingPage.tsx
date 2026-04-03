import { type FormEvent, useCallback, useEffect, useState } from 'react';
import type { LetsEncryptConfig, RoutingStatusResponse, RoutingTableEntry } from '../lib/api';
import {
  fetchLetsEncryptConfig,
  fetchRoutingStatus,
  fetchRoutingTable,
  getToken,
  setLetsEncryptConfig,
} from '../lib/api';
import ConnectScreen from './ConnectScreen';

export default function RoutingPage() {
  const [hasToken, setHasToken] = useState(() => Boolean(getToken()));

  if (!hasToken) {
    return <ConnectScreen onConnected={() => setHasToken(true)} />;
  }

  return <RoutingInner />;
}

function RoutingInner() {
  const [table, setTable] = useState<RoutingTableEntry[]>([]);
  const [status, setStatus] = useState<RoutingStatusResponse | null>(null);
  const [tlsConfig, setTlsConfig] = useState<LetsEncryptConfig | null>(null);
  const [emailDraft, setEmailDraft] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const [nextTable, nextStatus, nextTlsConfig] = await Promise.all([
        fetchRoutingTable(),
        fetchRoutingStatus(),
        fetchLetsEncryptConfig(),
      ]);

      setTable(nextTable);
      setStatus(nextStatus);
      setTlsConfig(nextTlsConfig);
      setEmailDraft(nextTlsConfig.email ?? '');
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to load routing overview.');
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

  if (loading) {
    return (
      <div className="panel loading-state">
        <span className="loading-spinner" />
        <span>Loading routing overview…</span>
      </div>
    );
  }

  if (!status || !tlsConfig) {
    return (
      <div className="panel error-state">
        Failed to load routing overview: {error ?? 'unknown error'}
      </div>
    );
  }

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

      {notice ? <p className="callout callout-success">{notice}</p> : null}
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
          <form onSubmit={handleSaveEmail} className="stack-md">
            <div className="form-group">
              <label className="form-label" htmlFor="le-email">
                Account email
              </label>
              <input
                id="le-email"
                className="input"
                type="email"
                value={emailDraft}
                onChange={(event) => setEmailDraft(event.target.value)}
                placeholder="ops@example.com"
              />
            </div>
            <div className="form-actions">
              <button className="btn btn-primary" type="submit" disabled={busy === 'tls-email'}>
                {busy === 'tls-email' ? 'Saving…' : 'Save email'}
              </button>
            </div>
          </form>
          <p className="text-muted">
            {tlsConfig.configured
              ? `Current email: ${tlsConfig.email}`
              : 'No global Let’s Encrypt email has been configured yet.'}
          </p>
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
                  <td className="font-mono">{app.proxy_config_present ? 'Present' : 'Missing'}</td>
                  <td>{app.issues.length === 0 ? 'None' : app.issues.join(' | ')}</td>
                </tr>
              ))}
            </tbody>
          </table>
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
