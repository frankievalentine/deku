import { type SubmitEvent, useCallback, useEffect, useMemo, useState } from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import {
  checkDaemonHealth,
  clearToken,
  fetchApps,
  fetchLetsEncryptConfig,
  fetchManagedServices,
  fetchObjectStoreConfig,
  fetchPlugins,
  fetchSshKeys,
  rotateDashboardToken,
  setLetsEncryptConfig,
  setToken,
} from '../lib/api';
import { copyText, showToast } from '../lib/shell';
import ConnectScreen from './ConnectScreen';
import Icon from './Icon';

interface SettingsState {
  totalApps: number;
  tlsEmail: string | null;
  tlsConfigured: boolean;
  objectStoreConfigured: boolean;
  objectStoreProvider: string | null;
  sshKeyCount: number;
  pluginCount: number;
  totalServices: number;
}
const EMPTY_SETTINGS_STATE: SettingsState = {
  totalApps: 0,
  tlsEmail: null,
  tlsConfigured: false,
  objectStoreConfigured: false,
  objectStoreProvider: null,
  sshKeyCount: 0,
  pluginCount: 0,
  totalServices: 0,
};

export default function SettingsPage() {
  const tokenAccess = useTokenAccess();

  if (tokenAccess === 'unknown') {
    return null;
  }

  if (tokenAccess === 'locked') {
    return <ConnectScreen />;
  }

  return <SettingsInner />;
}

function SettingsInner() {
  const [state, setState] = useState<SettingsState>(EMPTY_SETTINGS_STATE);
  const [emailDraft, setEmailDraft] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [health, setHealth] = useState<'online' | 'offline' | 'checking'>('checking');
  const [latestToken, setLatestToken] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const [apps, tls, objectStore, sshKeys, plugins, postgres, redis, mysql] = await Promise.all([
        fetchApps(),
        fetchLetsEncryptConfig(),
        fetchObjectStoreConfig(),
        fetchSshKeys(),
        fetchPlugins(),
        fetchManagedServices('postgres'),
        fetchManagedServices('redis'),
        fetchManagedServices('mysql'),
      ]);

      setState({
        totalApps: apps.length,
        tlsEmail: tls.email,
        tlsConfigured: tls.configured,
        objectStoreConfigured: objectStore.configured,
        objectStoreProvider: objectStore.object_store?.provider ?? null,
        sshKeyCount: sshKeys.length,
        pluginCount: plugins.length,
        totalServices: postgres.length + redis.length + mysql.length,
      });
      setEmailDraft(tls.email ?? '');
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to load dashboard settings.'
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
    setLatestToken(window.sessionStorage.getItem('deku_rotated_token'));
  }, [load]);

  useEffect(() => {
    let active = true;
    const timer = window.setInterval(() => {
      void pollHealth();
    }, 30_000);

    async function pollHealth() {
      const isOnline = await checkDaemonHealth();
      if (active) {
        setHealth(isOnline ? 'online' : 'offline');
      }
    }

    void pollHealth();

    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  const connectionSummary = useMemo(() => {
    if (health === 'online') return 'Connected and reachable';
    if (health === 'offline') return 'Token present, daemon unreachable';
    return 'Checking daemon health';
  }, [health]);

  async function handleSaveEmail(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const email = emailDraft.trim();

    if (!email) {
      showToast({
        title: 'Email required',
        description: 'Enter a Let’s Encrypt account email before saving.',
        variant: 'warning',
      });
      return;
    }

    try {
      setBusy('tls-email');
      await setLetsEncryptConfig(email);
      await load();
      showToast({
        title: 'TLS email saved',
        description: `Global Let's Encrypt email updated to ${email}.`,
        variant: 'success',
      });
    } catch (error) {
      showToast({
        title: 'Unable to save TLS email',
        description: error instanceof Error ? error.message : 'Saving failed.',
        variant: 'error',
      });
    } finally {
      setBusy(null);
    }
  }

  async function handleRotateToken() {
    const confirmed = window.confirm(
      'Rotate the dashboard token? Existing browser sessions will stop working.'
    );
    if (!confirmed) return;

    try {
      setBusy('rotate-token');
      const payload = await rotateDashboardToken();
      if (!payload?.token) {
        throw new Error('Daemon did not return a replacement token.');
      }

      setToken(payload.token);
      window.sessionStorage.setItem('deku_rotated_token', payload.token);
      setLatestToken(payload.token);
      showToast({
        title: 'Access token rotated',
        description:
          'Copy the replacement token now. It will only be shown again after another rotation.',
        variant: 'success',
      });
    } catch (error) {
      showToast({
        title: 'Unable to rotate token',
        description: error instanceof Error ? error.message : 'Token rotation failed.',
        variant: 'error',
      });
    } finally {
      setBusy(null);
    }
  }

  async function handleCopyToken() {
    if (!latestToken) return;

    const copied = await copyText(latestToken);
    showToast({
      title: copied ? 'Token copied' : 'Copy unavailable',
      description: copied
        ? 'The rotated dashboard token is now in your clipboard.'
        : 'Clipboard access is unavailable in this browser.',
      variant: copied ? 'success' : 'warning',
    });
  }

  function handleDisconnect() {
    clearToken();
    window.sessionStorage.removeItem('deku_rotated_token');
    setLatestToken(null);
    showToast({
      title: 'Logged out successfully',
      description: 'The stored dashboard token was removed from this browser.',
      variant: 'success',
    });
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <p className="eyebrow">Settings</p>
          <h1 className="page-title">Global dashboard configuration</h1>
          <p className="page-copy">
            Keep token security, Let’s Encrypt settings, and dashboard-wide integrations in one
            place.
          </p>
        </div>
        <div className="metrics-grid">
          <Metric
            label="Connection"
            value={health === 'online' ? 'Online' : health === 'offline' ? 'Offline' : 'Checking'}
          />
          <Metric label="Apps" value={String(state.totalApps)} />
          <Metric label="Services" value={String(state.totalServices)} />
          <Metric label="TLS Email" value={state.tlsConfigured ? 'Configured' : 'Missing'} />
        </div>
      </section>

      {error ? <p className="callout callout-danger">{error}</p> : null}

      <section className="settings-grid">
        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Security</p>
            <h2 className="section-title">Connection and access token</h2>
          </div>

          <div className="data-grid">
            <div>
              <dt>Status</dt>
              <dd>{connectionSummary}</dd>
            </div>
            <div>
              <dt>Stored in browser</dt>
              <dd>Yes</dd>
            </div>
          </div>

          <div className="form-actions token-action-row">
            <button
              type="button"
              className="btn btn-primary shell-action-button"
              disabled={busy !== null || loading}
              onClick={() => {
                void handleRotateToken();
              }}
            >
              <Icon
                name="rotate"
                size={16}
                className={busy === 'rotate-token' ? 'spin' : undefined}
              />
              {busy === 'rotate-token' ? (
                <>
                  <span className="loading-spinner" />
                  Rotating…
                </>
              ) : (
                <span>Rotate token</span>
              )}
            </button>
            <button
              type="button"
              className="btn btn-secondary shell-action-button"
              onClick={handleDisconnect}
            >
              <Icon name="disconnect" size={16} />
              <span>Disconnect</span>
            </button>
          </div>

          {latestToken ? (
            <div className="settings-token-preview">
              <div className="stack-sm">
                <p className="eyebrow">Latest rotated token</p>
                <code className="font-mono">{latestToken}</code>
              </div>
              <div className="form-actions">
                <button
                  type="button"
                  className="btn btn-secondary"
                  onClick={() => void handleCopyToken()}
                >
                  <Icon name="copy" size={16} />
                  <span>Copy token</span>
                </button>
              </div>
            </div>
          ) : (
            <p className="text-muted">No newly rotated token is cached in this browser session.</p>
          )}
        </article>

        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">TLS</p>
            <h2 className="section-title">Let’s Encrypt account email</h2>
            <p className="page-copy">
              This email is used for global certificate operations across routed apps.
            </p>
          </div>

          <form onSubmit={handleSaveEmail} className="stack-md">
            <div className="form-group">
              <label className="form-label" htmlFor="settings-le-email">
                Account email
              </label>
              <input
                id="settings-le-email"
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
                <Icon name="settings" size={16} />
                {busy === 'tls-email' ? (
                  <>
                    <span className="loading-spinner" />
                    Saving…
                  </>
                ) : (
                  <span>Save email</span>
                )}
              </button>
            </div>
          </form>

          <p className="text-muted">
            {state.tlsConfigured
              ? `Current email: ${state.tlsEmail}`
              : 'No global Let’s Encrypt email has been configured yet.'}
          </p>
        </article>

        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Integrations</p>
            <h2 className="section-title">Storage and services</h2>
          </div>

          <div className="summary-list">
            <SummaryLink
              href="/object-store"
              icon="object-store"
              title="Object Store"
              description={
                state.objectStoreConfigured
                  ? `Configured${state.objectStoreProvider ? ` with ${state.objectStoreProvider}` : ''}`
                  : 'Not configured'
              }
            />
            <SummaryLink
              href="/services"
              icon="services"
              title="Managed Services"
              description={`${state.totalServices} configured services`}
            />
          </div>
        </article>

        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Access</p>
            <h2 className="section-title">SSH keys and plugins</h2>
          </div>

          <div className="summary-list">
            <SummaryLink
              href="/ssh-keys"
              icon="ssh-keys"
              title="SSH Keys"
              description={`${state.sshKeyCount} keys loaded for access`}
            />
            <SummaryLink
              href="/plugins"
              icon="plugins"
              title="Plugins"
              description={`${state.pluginCount} loaded plugins`}
            />
          </div>
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

function SummaryLink({
  href,
  icon,
  title,
  description,
}: {
  href: string;
  icon: 'object-store' | 'services' | 'ssh-keys' | 'plugins';
  title: string;
  description: string;
}) {
  return (
    <a className="summary-link-card" href={href}>
      <span className="summary-link-icon">
        <Icon name={icon} size={18} />
      </span>
      <span className="summary-link-copy">
        <strong>{title}</strong>
        <small>{description}</small>
      </span>
      <Icon name="open" size={16} className="summary-link-arrow" />
    </a>
  );
}
