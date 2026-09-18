import { type SubmitEvent, useEffect, useState } from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import type { LetsEncryptConfig, RoutingStatusResponse } from '../lib/api';
import {
  getFirstQueryError,
  useLetsEncryptConfigQuery,
  useRoutingStatusQuery,
  useSetLetsEncryptConfigMutation,
} from '../lib/query';
import { showToast } from '../lib/shell';
import AcmeSettingsPanel from './AcmeSettingsPanel';
import ConnectScreen from './ConnectScreen';
import Icon from './Icon';
import Spinner from './Spinner';
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
  const statusQuery = useRoutingStatusQuery({ refetchInterval: 30_000 });
  const tlsConfigQuery = useLetsEncryptConfigQuery();
  const setLetsEncryptConfigMutation = useSetLetsEncryptConfigMutation();
  const status = statusQuery.data ?? EMPTY_ROUTING_STATUS;
  const tlsConfig = tlsConfigQuery.data ?? EMPTY_TLS_CONFIG;
  const error = getFirstQueryError([statusQuery.error, tlsConfigQuery.error], null);
  const loading = statusQuery.isPending;
  const loaded = statusQuery.data !== undefined;

  const [emailDraft, setEmailDraft] = useState('');
  const [emailError, setEmailError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    setEmailDraft(tlsConfig.email ?? '');
  }, [tlsConfig.email]);

  async function handleSaveEmail(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const email = emailDraft.trim();
    const nextEmailError = validateEmail(email);

    if (nextEmailError) {
      setEmailError(nextEmailError);
      document.getElementById('routing-le-email')?.focus();
      return;
    }

    try {
      setBusy('tls-email');
      setEmailError(null);
      await setLetsEncryptConfigMutation.mutateAsync(email);
      showToast({
        title: 'TLS email saved',
        description: `Global Let's Encrypt email updated to ${email}.`,
        variant: 'success',
      });
    } catch (nextError) {
      showToast({
        title: 'Unable to save TLS email',
        description: nextError instanceof Error ? nextError.message : 'Saving failed.',
        variant: 'error',
      });
    } finally {
      setBusy(null);
    }
  }

  function retryAll() {
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
          <h1 className="page-title">Domains and certificates</h1>
          <p className="page-copy">
            Domains, HTTPS certificates, and how traffic reaches each app.
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
          <div className="stack-sm">
            <p className="eyebrow">Proxy</p>
            <h2 className="section-title">Proxy configuration</h2>
          </div>
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
          <div className="stack-sm">
            <p className="eyebrow">Certificates</p>
            <h2 className="section-title">Let’s Encrypt account email</h2>
            <p className="page-copy">
              Used for certificate operations across every routed app on this host.
            </p>
          </div>

          <form onSubmit={handleSaveEmail} className="stack-md" noValidate>
            <div className="form-group">
              <label className="form-label" htmlFor="routing-le-email">
                Account email
              </label>
              <input
                id="routing-le-email"
                className="input"
                type="email"
                value={emailDraft}
                onChange={(event) => {
                  setEmailDraft(event.target.value);
                  if (emailError) setEmailError(null);
                }}
                placeholder="ops@example.com"
                disabled={busy !== null}
                autoComplete="email"
                spellCheck={false}
                required
                aria-invalid={emailError ? true : undefined}
                aria-describedby={emailError ? 'routing-le-email-error' : undefined}
              />
              {emailError ? (
                <p id="routing-le-email-error" className="text-danger">
                  {emailError}
                </p>
              ) : null}
            </div>
            <div className="form-actions">
              <button className="btn btn-primary" type="submit" disabled={busy !== null}>
                <Icon name="settings" size={16} />
                {busy === 'tls-email' ? <span className="loading-spinner" /> : null}
                <span>Save email</span>
              </button>
            </div>
          </form>

          <p className="text-muted">
            {tlsConfig.configured
              ? `Current email: ${tlsConfig.email}`
              : 'No account email is set. It is optional: the certificate authority uses it to reach you about the account, and certificates are issued without one.'}
          </p>
        </article>
      </section>

      <AcmeSettingsPanel />

      <article className="panel stack-md">
        <div className="stack-sm">
          <p className="eyebrow">Traffic</p>
          <h2 className="section-title">Routes</h2>
          <p className="page-copy">
            Every app's domains, environment and preview hostnames, upstreams, and routing health.
          </p>
        </div>

        {status.apps.length === 0 ? (
          <p className="text-muted">
            No routes are published yet. Add a domain to an app and deploy it to publish one.
          </p>
        ) : (
          <TableScroll>
            <table className="table">
              <caption className="sr-only">
                Routing table mapping each app to its domains, hostnames, and upstreams
              </caption>
              <thead>
                <tr>
                  <th scope="col">App</th>
                  <th scope="col">Domains</th>
                  <th scope="col">Environment and preview hostnames</th>
                  <th scope="col">Upstreams</th>
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
                    <td className="font-mono">
                      {app.domains.length === 0 ? 'None' : app.domains.join(', ')}
                    </td>
                    <td className="font-mono">
                      {app.hostnames.length === 0
                        ? 'None'
                        : app.hostnames
                            .map((hostname) =>
                              hostname.deployment_id
                                ? `${hostname.hostname} (preview, ${hostname.environment})`
                                : `${hostname.hostname} (${hostname.environment})`
                            )
                            .join(', ')}
                    </td>
                    <td className="font-mono">
                      {app.upstreams.length === 0
                        ? 'None'
                        : app.upstreams
                            .map((upstream) => `${upstream.host}:${upstream.port}`)
                            .join(', ')}
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

function validateEmail(email: string): string | null {
  if (!email) {
    return 'Enter the account email for Let’s Encrypt.';
  }

  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
    return 'Enter a valid email address, such as ops@example.com.';
  }

  return null;
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
