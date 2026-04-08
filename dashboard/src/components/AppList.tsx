import { type SubmitEvent, useCallback, useEffect, useState } from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import { type App, createApp, deleteApp, fetchApps } from '../lib/api';
import { showToast } from '../lib/shell';
import ConfirmModal from './ConfirmModal';
import ConnectScreen from './ConnectScreen';
import Icon from './Icon';
import StatusBadge from './StatusBadge';

function formatRelativeTime(value: string): string {
  const date = new Date(value);
  const diff = Date.now() - date.getTime();
  const minutes = Math.floor(diff / 60_000);
  const hours = Math.floor(diff / 3_600_000);
  const days = Math.floor(diff / 86_400_000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  return `${days}d ago`;
}

export default function AppList() {
  const tokenAccess = useTokenAccess();

  if (tokenAccess === 'unknown') {
    return null;
  }

  if (tokenAccess === 'locked') {
    return <ConnectScreen />;
  }

  return <AppListInner />;
}

function AppListInner() {
  const [apps, setApps] = useState<App[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [newAppName, setNewAppName] = useState('');
  const [createError, setCreateError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);

  const loadApps = useCallback(async () => {
    try {
      setLoadError(null);
      const nextApps = await fetchApps();
      setApps(nextApps);
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : 'Unable to load apps.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadApps();
    const timer = window.setInterval(() => {
      void loadApps();
    }, 20_000);
    return () => window.clearInterval(timer);
  }, [loadApps]);

  useEffect(() => {
    const query = new URLSearchParams(window.location.search);
    if (query.get('create') === '1') {
      setShowCreate(true);
      query.delete('create');
      const next = query.toString();
      window.history.replaceState({}, '', next ? `/?${next}` : '/');
    }
  }, []);

  async function handleCreate(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = newAppName.trim();
    if (!name) return;

    try {
      setSubmitting(true);
      await createApp(name);
      setShowCreate(false);
      setNewAppName('');
      setCreateError(null);
      await loadApps();
      showToast({
        title: 'App created',
        description: `${name} is now available in the fleet.`,
        variant: 'success',
      });
    } catch (error) {
      setCreateError(error instanceof Error ? error.message : 'Unable to create app.');
    } finally {
      setSubmitting(false);
    }
  }

  async function handleDelete() {
    if (!deleteTarget) return;
    try {
      setDeleting(deleteTarget);
      await deleteApp(deleteTarget);
      setDeleteTarget(null);
      await loadApps();
      showToast({
        title: 'App deleted',
        description: `${deleteTarget} was removed from the dashboard.`,
        variant: 'success',
      });
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : 'Unable to delete app.');
    } finally {
      setDeleting(null);
    }
  }

  if (loadError && !loading) {
    return <div className="panel error-state">Failed to load apps: {loadError}</div>;
  }

  const liveApps = apps.filter((app) => app.status === 'deployed').length;
  const lockedApps = apps.filter((app) => app.locked).length;
  const tlsApps = apps.filter((app) => app.tls_enabled).length;

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <p className="eyebrow">Mission control</p>
          <h1 className="page-title">Application fleet</h1>
          <p className="page-copy">
            Track deploy readiness, jump into per-app controls, and use the command palette to move
            across the dashboard without losing context.
          </p>
        </div>
        <div className="metrics-grid">
          <Metric label="Total apps" value={String(apps.length)} />
          <Metric label="Live" value={String(liveApps)} />
          <Metric label="Locked" value={String(lockedApps)} />
          <Metric label="TLS enabled" value={String(tlsApps)} />
        </div>
      </section>

      <div className="page-header">
        <div className="stack-sm">
          <p className="eyebrow">Fleet actions</p>
          <h2 className="section-title">Manage apps</h2>
        </div>
        <div className="cluster">
          <a className="btn btn-secondary" href="/settings">
            <Icon name="settings" size={16} />
            <span>Settings</span>
          </a>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => {
              setShowCreate(true);
              setCreateError(null);
            }}
          >
            <Icon name="create" size={16} />
            <span>New app</span>
          </button>
        </div>
      </div>

      {showCreate && (
        <div className="panel stack-md panel-accent">
          <form onSubmit={handleCreate} className="stack-md">
            <div className="form-group">
              <label htmlFor="new-app-name" className="form-label">
                App name
              </label>
              <input
                id="new-app-name"
                type="text"
                className="input"
                placeholder="my-app"
                value={newAppName}
                onChange={(event) => setNewAppName(event.target.value)}
                pattern="[a-z0-9][a-z0-9-]*"
                title="Lowercase letters, numbers, and hyphens only"
                autoComplete="off"
              />
            </div>
            {createError ? <p className="callout callout-danger">{createError}</p> : null}
            <div className="form-actions">
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => {
                  setShowCreate(false);
                  setNewAppName('');
                  setCreateError(null);
                }}
              >
                Cancel
              </button>
              <button type="submit" className="btn btn-primary" disabled={submitting}>
                <Icon name="create" size={16} />
                <span>{submitting ? 'Creating…' : 'Create'}</span>
              </button>
            </div>
          </form>
        </div>
      )}

      {apps.length === 0 ? (
        <div className="panel empty-state">
          <p>No apps yet. Create one to seed the deck.</p>
        </div>
      ) : (
        <div className="app-grid">
          {apps.map((app) => (
            <article key={app.id} className="panel app-card">
              <div className="cluster justify-between align-start">
                <div className="stack-sm">
                  <a href={`/app?name=${encodeURIComponent(app.name)}`} className="app-name-link">
                    {app.name}
                  </a>
                  <p className="eyebrow">Created {formatRelativeTime(app.created_at)}</p>
                </div>
                <StatusBadge status={app.status} />
              </div>

              <dl className="data-grid">
                <div>
                  <dt>Lock state</dt>
                  <dd>{app.locked ? 'Locked' : 'Writable'}</dd>
                </div>
                <div>
                  <dt>TLS</dt>
                  <dd>{app.tls_enabled ? 'Enabled' : 'Off'}</dd>
                </div>
                <div>
                  <dt>App ID</dt>
                  <dd className="font-mono">{app.id.slice(0, 8)}</dd>
                </div>
                <div>
                  <dt>Status code</dt>
                  <dd className="font-mono">{app.status}</dd>
                </div>
              </dl>

              <div className="cluster justify-between align-center">
                <div className="cluster">
                  <a
                    href={`/app?name=${encodeURIComponent(app.name)}`}
                    className="btn btn-secondary btn-sm"
                  >
                    Open app
                  </a>
                  <a
                    href={`/deployments?app=${encodeURIComponent(app.name)}`}
                    className="btn btn-ghost btn-sm"
                  >
                    Deployments
                  </a>
                </div>
                <button
                  type="button"
                  className="btn btn-danger btn-sm"
                  onClick={() => setDeleteTarget(app.name)}
                  disabled={deleting === app.name}
                  aria-label={`Delete ${app.name}`}
                >
                  {deleting === app.name ? 'Deleting…' : 'Delete'}
                </button>
              </div>
            </article>
          ))}
        </div>
      )}

      <ConfirmModal
        open={deleteTarget !== null}
        title={`Delete ${deleteTarget ?? 'app'}?`}
        description={
          deleteTarget
            ? `This permanently removes the app record and its attached runtime metadata for ${deleteTarget}. This action cannot be undone.`
            : ''
        }
        confirmLabel="Delete app"
        cancelLabel="Keep app"
        busy={deleting !== null}
        onConfirm={() => {
          void handleDelete();
        }}
        onClose={() => setDeleteTarget(null)}
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
