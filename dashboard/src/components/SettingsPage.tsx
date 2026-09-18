import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useMemo, useState } from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import { clearToken, getDashboardBuildVersion, setToken } from '../lib/api';
import {
  getDashboardQueryClient,
  getErrorMessage,
  useDaemonHealthQuery,
  useRotateDashboardTokenMutation,
  useSettingsSummaryQuery,
  useVersionStatusQuery,
} from '../lib/query';
import { copyText, showToast } from '../lib/shell';
import ConfirmModal from './ConfirmModal';
import ConnectScreen from './ConnectScreen';
import Icon from './Icon';
import PluginsPanel from './PluginsPanel';
import Spinner from './Spinner';

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
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState<string | null>(null);
  const [latestToken, setLatestToken] = useState<string | null>(null);
  const [confirmRotate, setConfirmRotate] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const settingsQuery = useSettingsSummaryQuery();
  const healthQuery = useDaemonHealthQuery({ refetchInterval: 30_000 });
  const versionQuery = useVersionStatusQuery({ refetchInterval: 300_000 });
  const rotateDashboardTokenMutation = useRotateDashboardTokenMutation();
  const state = settingsQuery.data ?? EMPTY_SETTINGS_STATE;
  const loading = settingsQuery.isPending;
  const versionStatus = versionQuery.data;
  const versionError = versionQuery.error
    ? getErrorMessage(versionQuery.error, 'Unable to load release status.')
    : null;
  const dashboardBuildVersion = useMemo(() => getDashboardBuildVersion() ?? 'v0.1.12', []);
  const health =
    healthQuery.isPending || healthQuery.data === undefined
      ? 'checking'
      : healthQuery.data
        ? 'online'
        : 'offline';

  useEffect(() => {
    setLatestToken(window.sessionStorage.getItem('deku_rotated_token'));
  }, []);

  useEffect(() => {
    if (settingsQuery.error) {
      setError(getErrorMessage(settingsQuery.error, 'Unable to load dashboard settings.'));
      return;
    }

    setError(null);
  }, [settingsQuery.error]);

  const connectionSummary = useMemo(() => {
    if (health === 'online') return 'Connected and reachable';
    if (health === 'offline') return 'Token present, daemon unreachable';
    return 'Checking daemon health';
  }, [health]);

  const releaseSummary = useMemo(() => {
    if (versionError) return 'Unable to load release status';
    if (!versionStatus) return 'Checking for updates';
    if (versionStatus.status === 'error') return 'Unable to check GitHub releases';
    if (versionStatus.update_available) {
      return `Update available: ${versionStatus.current_version} -> ${versionStatus.latest_version}`;
    }
    return `Running the latest release (${versionStatus.current_version})`;
  }, [versionError, versionStatus]);

  async function handleRotateToken() {
    try {
      setBusy('rotate-token');
      const payload = await rotateDashboardTokenMutation.mutateAsync();
      if (!payload?.token) {
        throw new Error('Daemon did not return a replacement token.');
      }

      setToken(payload.token);
      window.sessionStorage.setItem('deku_rotated_token', payload.token);
      setLatestToken(payload.token);
      setConfirmRotate(false);
      await queryClient.invalidateQueries();
      showToast({
        title: 'Access token rotated',
        description:
          'Copy the replacement token now. It will only be shown again after another rotation.',
        variant: 'success',
      });
    } catch (error) {
      setConfirmRotate(false);
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
    getDashboardQueryClient().clear();
    setLatestToken(null);
    showToast({
      title: 'Logged out successfully',
      description: 'The stored dashboard token was removed from this browser.',
      variant: 'success',
    });
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <Spinner />
        <span>Loading dashboard settings…</span>
      </div>
    );
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <h1 className="page-title">Dashboard settings</h1>
          <p className="page-copy">Access token, release version, and plugins.</p>
        </div>
        <div className="metrics-grid">
          <Metric
            label="Connection"
            value={health === 'online' ? 'Online' : health === 'offline' ? 'Offline' : 'Checking'}
          />
          <Metric label="Apps" value={String(state.totalApps)} />
          <Metric label="Services" value={String(state.totalServices)} />
          <Metric label="TLS email" value={state.tlsConfigured ? 'Configured' : 'Missing'} />
        </div>
      </section>

      {error ? (
        <p className="callout callout-danger" role="alert">
          {error}
        </p>
      ) : null}

      <section className="settings-grid">
        <article className="panel stack-md">
          <h2 className="section-title">Connection and access token</h2>

          <dl className="data-grid">
            <div>
              <dt>Status</dt>
              <dd>{connectionSummary}</dd>
            </div>
            <div>
              <dt>Stored in browser</dt>
              <dd>Yes</dd>
            </div>
          </dl>

          <div className="form-actions settings-token-actions">
            <button
              type="button"
              className="btn btn-primary"
              disabled={busy !== null || loading}
              onClick={() => setConfirmRotate(true)}
            >
              <Icon
                name="rotate"
                size={16}
                className={busy === 'rotate-token' ? 'spin' : undefined}
              />
              <span>Rotate token</span>
            </button>
            <button type="button" className="btn btn-secondary" onClick={handleDisconnect}>
              <Icon name="disconnect" size={16} />
              <span>Disconnect</span>
            </button>
          </div>

          {latestToken ? (
            <div className="settings-token-preview">
              <div className="stack-sm">
                <h3 className="deploy-title">Newest rotated token</h3>
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
            <p className="text-muted">
              Rotated tokens are shown once. Rotate the token to generate a new one.
            </p>
          )}
        </article>

        <article className="panel stack-md">
          <h2 className="section-title">Version and updates</h2>

          <dl className="data-grid">
            <div>
              <dt>Installed</dt>
              <dd>{versionStatus?.current_version ?? 'Checking…'}</dd>
            </div>
            <div>
              <dt>Latest release</dt>
              <dd>{versionStatus?.latest_version ?? 'Unavailable'}</dd>
            </div>
            <div>
              <dt>Status</dt>
              <dd>{releaseSummary}</dd>
            </div>
            <div>
              <dt>Dashboard build</dt>
              <dd>{dashboardBuildVersion}</dd>
            </div>
          </dl>

          {versionStatus?.update_available ? (
            <p className="callout callout-warning">
              A newer Deku release is available. Upgrade from {versionStatus.current_version} to{' '}
              {versionStatus.latest_version}.
            </p>
          ) : null}

          {versionError ? <p className="callout callout-warning">{versionError}</p> : null}

          {versionStatus?.status === 'error' ? (
            <p className="callout callout-warning">
              Unable to check for updates right now.
              {versionStatus.error ? ` ${versionStatus.error}` : ''}
            </p>
          ) : null}

          {!versionStatus?.update_available && versionStatus?.status === 'ok' ? (
            <p className="text-muted">This host is already on the latest published Deku release.</p>
          ) : null}
        </article>
      </section>

      <PluginsPanel />

      <ConfirmModal
        open={confirmRotate}
        title="Rotate the access token?"
        description="Every browser session and CLI client using the current token loses access immediately. Copy the replacement token from this page after rotating."
        confirmLabel="Rotate token"
        cancelLabel="Keep current token"
        busy={busy === 'rotate-token'}
        onConfirm={() => {
          void handleRotateToken();
        }}
        onClose={() => {
          if (busy === null) setConfirmRotate(false);
        }}
      />
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
