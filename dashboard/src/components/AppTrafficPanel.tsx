import { type SubmitEvent, useCallback, useEffect, useState } from 'react';
import {
  type AppAuthMode,
  type AppAuthStatus,
  addRedirect,
  clearAppAuth,
  fetchAppAuth,
  fetchMaintenance,
  fetchRedirects,
  type MaintenanceState,
  type RedirectRecord,
  removeRedirect,
  setAppAuth,
  setMaintenance,
} from '../lib/api';
import ConfirmModal from './ConfirmModal';
import SelectField from './SelectField';
import Spinner from './Spinner';
import TableScroll from './TableScroll';

interface AppTrafficPanelProps {
  appName: string;
  locked: boolean;
  onAppRefresh?: () => void;
}

const REDIRECT_CODES = [
  { value: '301', label: '301 - Permanent' },
  { value: '302', label: '302 - Temporary' },
  { value: '307', label: '307 - Temporary, keep method' },
  { value: '308', label: '308 - Permanent, keep method' },
];

export default function AppTrafficPanel({ appName, locked, onAppRefresh }: AppTrafficPanelProps) {
  const [auth, setAuth] = useState<AppAuthStatus | null>(null);
  const [maintenance, setMaintenanceState] = useState<MaintenanceState | null>(null);
  const [redirects, setRedirects] = useState<RedirectRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [authMode, setAuthMode] = useState<AppAuthMode>('basic');
  const [authUsername, setAuthUsername] = useState('');
  const [authPassword, setAuthPassword] = useState('');
  const [authForwardUrl, setAuthForwardUrl] = useState('');
  const [confirmClearAuth, setConfirmClearAuth] = useState(false);

  const [maintenanceMessage, setMaintenanceMessage] = useState('');

  const [redirectSource, setRedirectSource] = useState('');
  const [redirectTarget, setRedirectTarget] = useState('');
  const [redirectCode, setRedirectCode] = useState('301');
  const [redirectToRemove, setRedirectToRemove] = useState<RedirectRecord | null>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const [nextAuth, nextMaintenance, nextRedirects] = await Promise.all([
        fetchAppAuth(appName),
        fetchMaintenance(appName),
        fetchRedirects(appName),
      ]);
      setAuth(nextAuth);
      setMaintenanceState(nextMaintenance);
      setRedirects(nextRedirects);
      setAuthMode(nextAuth.mode === 'forward' ? 'forward' : 'basic');
      setAuthUsername(nextAuth.username ?? '');
      setAuthForwardUrl(nextAuth.forward_url ?? '');
      setMaintenanceMessage(nextMaintenance.message ?? '');
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to load traffic settings.');
    } finally {
      setLoading(false);
    }
  }, [appName]);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleSaveAuth(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (locked) return;

    if (authMode === 'basic' && (!authUsername.trim() || !authPassword)) {
      setError('Enter a username and password for basic authentication.');
      return;
    }
    if (authMode === 'forward' && !authForwardUrl.trim()) {
      setError('Enter the URL that should receive forwarded authentication requests.');
      return;
    }

    try {
      setBusy('auth-save');
      setError(null);
      setNotice(null);
      await setAppAuth(appName, {
        mode: authMode,
        username: authMode === 'basic' ? authUsername.trim() : null,
        password: authMode === 'basic' ? authPassword : null,
        forward_url: authMode === 'forward' ? authForwardUrl.trim() : null,
      });
      setAuthPassword('');
      await load();
      setNotice('Authentication saved. New requests are checked against it.');
      onAppRefresh?.();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to save authentication.');
    } finally {
      setBusy(null);
    }
  }

  async function handleClearAuth() {
    try {
      setBusy('auth-clear');
      setError(null);
      setNotice(null);
      await clearAppAuth(appName);
      setConfirmClearAuth(false);
      await load();
      setNotice('Authentication removed. The app is publicly reachable again.');
      onAppRefresh?.();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to remove authentication.');
    } finally {
      setBusy(null);
    }
  }

  async function handleSaveMaintenance(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (locked || !maintenance) return;

    const nextEnabled = !maintenance.enabled;
    try {
      setBusy('maintenance');
      setError(null);
      setNotice(null);
      await setMaintenance(appName, {
        enabled: nextEnabled,
        message: nextEnabled ? maintenanceMessage.trim() || null : null,
      });
      await load();
      setNotice(
        nextEnabled
          ? 'Maintenance mode is on. Visitors see a 503 until you turn it off.'
          : 'Maintenance mode is off. Visitors reach the app normally.'
      );
      onAppRefresh?.();
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to update maintenance mode.'
      );
    } finally {
      setBusy(null);
    }
  }

  async function handleAddRedirect(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (locked) return;

    const source = redirectSource.trim();
    const target = redirectTarget.trim();
    if (!source || !target) {
      setError('Enter both a path to redirect and where it should go.');
      return;
    }

    try {
      setBusy('redirect-add');
      setError(null);
      setNotice(null);
      await addRedirect(appName, {
        source_path: source,
        target,
        code: Number(redirectCode),
      });
      setRedirectSource('');
      setRedirectTarget('');
      await load();
      setNotice(`Redirects from ${source} to ${target}.`);
      onAppRefresh?.();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to add the redirect.');
    } finally {
      setBusy(null);
    }
  }

  async function handleRemoveRedirect() {
    if (!redirectToRemove) return;
    const redirect = redirectToRemove;
    try {
      setBusy(`redirect-remove-${redirect.id}`);
      setError(null);
      setNotice(null);
      await removeRedirect(appName, redirect.id);
      setRedirectToRemove(null);
      await load();
      setNotice(`Removed the redirect for ${redirect.source_path}.`);
      onAppRefresh?.();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to remove the redirect.');
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return (
      <article className="panel stack-md">
        <div className="loading-state">
          <Spinner />
          <span>Loading traffic settings…</span>
        </div>
      </article>
    );
  }

  const authConfigured = auth?.configured ?? false;

  return (
    <div className="stack-lg">
      {notice ? <p className="callout callout-success">{notice}</p> : null}
      {error ? <p className="callout callout-danger">{error}</p> : null}

      <article className="panel stack-md">
        <div className="panel-heading">
          <div className="stack-sm panel-heading-copy">
            <p className="eyebrow">Access</p>
            <h2 className="section-title">Who can reach the app</h2>
            <p className="page-copy">
              Require a username and password, or ask another service to decide. Changes apply to
              new requests as soon as they are saved.
            </p>
          </div>
          <span className="inventory-summary">
            {authConfigured ? (auth?.mode === 'forward' ? 'Forwarded' : 'Password') : 'Open'}
          </span>
        </div>

        <form onSubmit={handleSaveAuth} className="stack-md" noValidate>
          <div className="form-group">
            <label className="form-label" htmlFor="traffic-auth-mode">
              Protection
            </label>
            <SelectField
              id="traffic-auth-mode"
              value={authMode}
              onChange={(next) => setAuthMode(next as AppAuthMode)}
              disabled={locked || busy !== null}
              options={[
                { value: 'basic', label: 'Password' },
                { value: 'forward', label: 'Another service' },
              ]}
            />
          </div>

          {authMode === 'basic' ? (
            <div className="panel-grid">
              <div className="form-group">
                <label className="form-label" htmlFor="traffic-auth-username">
                  Username
                </label>
                <input
                  id="traffic-auth-username"
                  className="input"
                  autoComplete="off"
                  value={authUsername}
                  onChange={(event) => setAuthUsername(event.target.value)}
                  disabled={locked || busy !== null}
                />
              </div>
              <div className="form-group">
                <label className="form-label" htmlFor="traffic-auth-password">
                  Password
                </label>
                <input
                  id="traffic-auth-password"
                  className="input"
                  type="password"
                  autoComplete="new-password"
                  placeholder={authConfigured && authMode === 'basic' ? 'Leave blank to keep' : ''}
                  value={authPassword}
                  onChange={(event) => setAuthPassword(event.target.value)}
                  disabled={locked || busy !== null}
                />
              </div>
            </div>
          ) : (
            <div className="form-group">
              <label className="form-label" htmlFor="traffic-auth-forward-url">
                Forward to
              </label>
              <input
                id="traffic-auth-forward-url"
                className="input"
                inputMode="url"
                placeholder="https://auth.example.com/verify"
                value={authForwardUrl}
                onChange={(event) => setAuthForwardUrl(event.target.value)}
                disabled={locked || busy !== null}
              />
            </div>
          )}

          <div className="form-actions">
            <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
              {busy === 'auth-save' ? <span className="loading-spinner" /> : null}
              <span>{authConfigured ? 'Update access' : 'Require sign-in'}</span>
            </button>
            <button
              type="button"
              className="btn btn-danger"
              onClick={() => setConfirmClearAuth(true)}
              disabled={locked || busy !== null || !authConfigured}
            >
              Remove protection
            </button>
          </div>
        </form>
      </article>

      <article className="panel stack-md">
        <div className="panel-heading">
          <div className="stack-sm panel-heading-copy">
            <p className="eyebrow">Availability</p>
            <h2 className="section-title">Maintenance mode</h2>
            <p className="page-copy">
              Serve a temporary page instead of the app while you deploy or migrate. Turn it off to
              send visitors back to the app.
            </p>
          </div>
          <span className="inventory-summary">{maintenance?.enabled ? 'On' : 'Off'}</span>
        </div>

        <form onSubmit={handleSaveMaintenance} className="stack-md" noValidate>
          <div className="form-group">
            <label className="form-label" htmlFor="traffic-maintenance-message">
              Message shown to visitors
            </label>
            <input
              id="traffic-maintenance-message"
              className="input"
              placeholder="Back shortly - deploying a new version"
              value={maintenanceMessage}
              onChange={(event) => setMaintenanceMessage(event.target.value)}
              disabled={locked || busy !== null}
            />
            <p className="text-muted">Optional. Only shown while maintenance mode is on.</p>
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
              {busy === 'maintenance' ? <span className="loading-spinner" /> : null}
              <span>{maintenance?.enabled ? 'Turn maintenance off' : 'Turn maintenance on'}</span>
            </button>
          </div>
        </form>
      </article>

      <article className="panel stack-md">
        <div className="panel-heading">
          <div className="stack-sm panel-heading-copy">
            <p className="eyebrow">Redirects</p>
            <h2 className="section-title">Send old paths somewhere new</h2>
            <p className="page-copy">
              Redirect a path on this app to another path or hostname. Use a permanent code only
              when the change lasts.
            </p>
          </div>
          <span className="inventory-summary">{redirects.length} configured</span>
        </div>

        <form onSubmit={handleAddRedirect} className="stack-md" noValidate>
          <div className="panel-grid">
            <div className="form-group">
              <label className="form-label" htmlFor="traffic-redirect-source">
                From path
              </label>
              <input
                id="traffic-redirect-source"
                className="input"
                placeholder="/old-pricing"
                value={redirectSource}
                onChange={(event) => setRedirectSource(event.target.value)}
                disabled={locked || busy !== null}
              />
            </div>
            <div className="form-group">
              <label className="form-label" htmlFor="traffic-redirect-target">
                To
              </label>
              <input
                id="traffic-redirect-target"
                className="input"
                placeholder="/pricing or https://example.com/pricing"
                value={redirectTarget}
                onChange={(event) => setRedirectTarget(event.target.value)}
                disabled={locked || busy !== null}
              />
            </div>
            <div className="form-group">
              <label className="form-label" htmlFor="traffic-redirect-code">
                Type
              </label>
              <SelectField
                id="traffic-redirect-code"
                value={redirectCode}
                onChange={setRedirectCode}
                disabled={locked || busy !== null}
                options={REDIRECT_CODES}
              />
            </div>
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
              {busy === 'redirect-add' ? <span className="loading-spinner" /> : null}
              <span>Add redirect</span>
            </button>
          </div>
        </form>

        {redirects.length === 0 ? (
          <p className="text-muted">No redirects yet.</p>
        ) : (
          <TableScroll>
            <table className="table">
              <caption className="sr-only">Redirects configured for this app</caption>
              <thead>
                <tr>
                  <th scope="col">From</th>
                  <th scope="col">To</th>
                  <th scope="col">Type</th>
                  <th scope="col">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {redirects.map((redirect) => (
                  <tr key={redirect.id}>
                    <td className="font-mono">{redirect.source_path}</td>
                    <td className="font-mono">{redirect.target}</td>
                    <td>{redirect.code}</td>
                    <td>
                      <button
                        className="btn btn-outline btn-danger-outline btn-sm"
                        type="button"
                        aria-label={`Remove redirect from ${redirect.source_path}`}
                        onClick={() => setRedirectToRemove(redirect)}
                        disabled={locked || busy !== null}
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
      </article>

      <ConfirmModal
        open={confirmClearAuth}
        title="Remove sign-in protection?"
        description={`Visitors will reach ${appName} without signing in.`}
        confirmLabel="Remove protection"
        busy={busy === 'auth-clear'}
        onClose={() => {
          if (busy !== 'auth-clear') setConfirmClearAuth(false);
        }}
        onConfirm={() => {
          void handleClearAuth();
        }}
      />

      <ConfirmModal
        open={redirectToRemove !== null}
        title="Remove this redirect?"
        description={
          redirectToRemove
            ? `${redirectToRemove.source_path} will stop redirecting to ${redirectToRemove.target}.`
            : ''
        }
        confirmLabel="Remove redirect"
        busy={redirectToRemove ? busy === `redirect-remove-${redirectToRemove.id}` : false}
        onClose={() => {
          if (!busy?.startsWith('redirect-remove-')) setRedirectToRemove(null);
        }}
        onConfirm={() => {
          void handleRemoveRedirect();
        }}
      />
    </div>
  );
}
