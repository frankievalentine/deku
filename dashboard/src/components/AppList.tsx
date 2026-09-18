import { useQueryClient } from '@tanstack/react-query';
import { type SubmitEvent, useEffect, useMemo, useRef, useState } from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import type { App } from '../lib/api';
import {
  getErrorMessage,
  prefetchAppDetail,
  useAppsQuery,
  useCreateAppMutation,
  useDeleteAppMutation,
} from '../lib/query';
import { showToast } from '../lib/shell';
import ConfirmModal from './ConfirmModal';
import ConnectScreen from './ConnectScreen';
import Icon from './Icon';
import StatusBadge from './StatusBadge';
import TableScroll from './TableScroll';

const APP_NAME_PATTERN = /^[a-z0-9][a-z0-9-]*$/;

function validateAppName(name: string): string | null {
  if (!name) {
    return 'Enter an app name to create the app.';
  }

  if (!APP_NAME_PATTERN.test(name)) {
    return 'Use lowercase letters, numbers, and hyphens; start with a letter or number.';
  }

  return null;
}

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
  const queryClient = useQueryClient();
  const [showCreate, setShowCreate] = useState(false);
  const [newAppName, setNewAppName] = useState('');
  const [createError, setCreateError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const prefetchedAppsRef = useRef<Set<string>>(new Set());
  const nameInputRef = useRef<HTMLInputElement | null>(null);
  const appsQuery = useAppsQuery({ refetchInterval: 20_000 });
  const createAppMutation = useCreateAppMutation();
  const deleteAppMutation = useDeleteAppMutation();
  const apps = appsQuery.data ?? [];
  const loading = appsQuery.isPending;
  const loadError = appsQuery.error
    ? getErrorMessage(appsQuery.error, 'Unable to load apps.')
    : null;

  useEffect(() => {
    const query = new URLSearchParams(window.location.search);
    if (query.get('create') === '1') {
      setShowCreate(true);
      query.delete('create');
      const next = query.toString();
      window.history.replaceState({}, '', next ? `/apps?${next}` : '/apps');
    }
  }, []);

  async function handleCreate(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = newAppName.trim();
    const nameProblem = validateAppName(name);
    if (nameProblem) {
      setCreateError(nameProblem);
      nameInputRef.current?.focus();
      return;
    }

    try {
      setSubmitting(true);
      setCreateError(null);
      await createAppMutation.mutateAsync(name);
      setShowCreate(false);
      setNewAppName('');
      setCreateError(null);
      showToast({
        title: 'App created',
        description: `${name} is now available in the fleet.`,
        variant: 'success',
      });
    } catch (error) {
      setCreateError(getErrorMessage(error, 'Unable to create app.'));
    } finally {
      setSubmitting(false);
    }
  }

  async function handleDelete() {
    if (!deleteTarget) return;
    try {
      setDeleting(deleteTarget);
      await deleteAppMutation.mutateAsync(deleteTarget);
      setDeleteTarget(null);
      showToast({
        title: 'App deleted',
        description: `${deleteTarget} was removed from the dashboard.`,
        variant: 'success',
      });
    } catch (error) {
      showToast({
        title: 'Unable to delete app',
        description: getErrorMessage(error, 'Deleting failed.'),
        variant: 'error',
      });
    } finally {
      setDeleting(null);
    }
  }

  function handlePrefetchAppDetail(appName: string) {
    if (prefetchedAppsRef.current.has(appName)) {
      return;
    }

    prefetchedAppsRef.current.add(appName);
    void prefetchAppDetail(queryClient, appName).catch(() => {
      prefetchedAppsRef.current.delete(appName);
    });
  }

  if (loadError && !loading) {
    return <div className="panel error-state">Failed to load apps: {loadError}</div>;
  }

  return (
    <div className="stack-lg">
      <header className="apps-header">
        <div className="apps-header-top">
          <div className="apps-header-copy">
            <p className="eyebrow">Apps</p>
            <h1 className="page-title">Apps</h1>
            <p className="page-copy">
              Open an app to deploy it, connect a domain, and watch it run.
            </p>
          </div>
          <div className="cluster">
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => {
                setShowCreate(true);
                setCreateError(null);
              }}
            >
              <Icon name="create" size={16} />
              <span>Create app</span>
            </button>
          </div>
        </div>
        <AppMetrics apps={apps} />
      </header>

      {showCreate && (
        <div className="panel stack-md panel-accent">
          <form onSubmit={handleCreate} className="stack-md" noValidate>
            <div className="form-group">
              <label htmlFor="new-app-name" className="form-label">
                App name
              </label>
              <input
                id="new-app-name"
                ref={nameInputRef}
                type="text"
                className="input"
                placeholder="my-app"
                value={newAppName}
                onChange={(event) => {
                  setNewAppName(event.target.value);
                  if (createError) setCreateError(null);
                }}
                aria-invalid={createError ? true : undefined}
                aria-describedby={createError ? 'new-app-name-error' : undefined}
                pattern="[a-z0-9][a-z0-9-]*"
                title="Use lowercase letters, numbers, and hyphens; start with a letter or number."
                autoComplete="off"
                spellCheck={false}
              />
              {createError ? (
                <p id="new-app-name-error" className="form-error">
                  {createError}
                </p>
              ) : null}
            </div>
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
                <span>{submitting ? 'Creating…' : 'Create app'}</span>
              </button>
            </div>
          </form>
        </div>
      )}

      {apps.length === 0 ? (
        <div className="panel empty-state">
          <p>No apps yet</p>
          <p className="text-muted">Create an app to deploy it and watch it run here.</p>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => {
              setShowCreate(true);
              setCreateError(null);
            }}
          >
            <Icon name="create" size={16} />
            <span>Create app</span>
          </button>
        </div>
      ) : (
        <div className="panel apps-table-card">
          <TableScroll>
            <table className="table apps-table">
              <caption className="sr-only">Apps in this Deku fleet</caption>
              <thead>
                <tr>
                  <th scope="col">App</th>
                  <th scope="col">Status</th>
                  <th scope="col">Created</th>
                  <th scope="col">Access</th>
                  <th scope="col">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {apps.map((app) => (
                  <tr key={app.id}>
                    <td>
                      <a
                        href={`/app?name=${encodeURIComponent(app.name)}`}
                        className="apps-row-name"
                        title={app.name}
                        onMouseEnter={() => handlePrefetchAppDetail(app.name)}
                        onFocus={() => handlePrefetchAppDetail(app.name)}
                      >
                        {app.name}
                      </a>
                    </td>
                    <td>
                      <StatusBadge status={app.status} size="sm" />
                    </td>
                    <td>
                      <span className="apps-row-time" title={formatAbsoluteTime(app.created_at)}>
                        {formatRelativeTime(app.created_at)}
                      </span>
                    </td>
                    <td>
                      <span className="apps-row-access">
                        <span>{app.locked ? 'Locked' : 'Writable'}</span>{' '}
                        <span aria-hidden="true">·</span>{' '}
                        <span className={`apps-tls${app.tls_enabled ? ' is-on' : ''}`}>
                          {app.tls_enabled ? 'TLS on' : 'TLS off'}
                        </span>
                      </span>
                    </td>
                    <td>
                      <div className="apps-row-actions">
                        <a
                          href={`/app?name=${encodeURIComponent(app.name)}`}
                          className="btn btn-secondary btn-sm"
                          onMouseEnter={() => handlePrefetchAppDetail(app.name)}
                          onFocus={() => handlePrefetchAppDetail(app.name)}
                        >
                          Open
                          <span className="sr-only"> {app.name}</span>
                        </a>
                        <a
                          href={`/deployments?app=${encodeURIComponent(app.name)}`}
                          className="btn btn-ghost btn-sm"
                        >
                          Deployments
                          <span className="sr-only"> for {app.name}</span>
                        </a>
                        <button
                          type="button"
                          className="btn btn-outline btn-danger-outline btn-sm"
                          onClick={() => setDeleteTarget(app.name)}
                          disabled={deleting === app.name}
                        >
                          {deleting === app.name ? 'Deleting…' : 'Delete'}
                          <span className="sr-only"> {app.name}</span>
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableScroll>
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

function AppMetrics({ apps }: { apps: App[] }) {
  const metrics = useMemo(
    () => [
      { label: 'Apps', value: apps.length },
      { label: 'Live', value: apps.filter((app) => app.status === 'deployed').length },
      { label: 'Locked', value: apps.filter((app) => app.locked).length },
      { label: 'TLS', value: apps.filter((app) => app.tls_enabled).length },
    ],
    [apps]
  );

  return (
    <ul className="apps-metrics">
      {metrics.map((metric) => (
        <li key={metric.label} className="apps-metric">
          <span className="apps-metric-value">{metric.value}</span>
          <span className="apps-metric-label">{metric.label}</span>
        </li>
      ))}
    </ul>
  );
}

function formatAbsoluteTime(value: string): string {
  return new Date(value).toLocaleString('en-US', {
    month: 'short',
    day: '2-digit',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}
