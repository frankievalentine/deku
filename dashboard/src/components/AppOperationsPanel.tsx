import { useState } from 'react';
import { getErrorMessage, useDeleteAppMutation } from '../lib/query';
import ConfirmModal from './ConfirmModal';

interface AppOperationsPanelProps {
  appId: string;
  appName: string;
  createdAt: string;
  locked: boolean;
  status: string;
}

export default function AppOperationsPanel({
  appId,
  appName,
  createdAt,
  locked,
  status,
}: AppOperationsPanelProps) {
  const deleteAppMutation = useDeleteAppMutation();
  const [confirmName, setConfirmName] = useState('');
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const readyToDelete = confirmName.trim() === appName;

  async function handleDelete() {
    try {
      setBusy('delete');
      setError(null);
      setNotice(null);
      await deleteAppMutation.mutateAsync(appName);
      setNotice(`Deleted app ${appName}. Redirecting to fleet view…`);
      window.setTimeout(() => {
        window.location.assign('/');
      }, 500);
    } catch (nextError) {
      setError(getErrorMessage(nextError, 'Unable to delete app.'));
    } finally {
      setBusy(null);
      setConfirmDelete(false);
    }
  }

  return (
    <section className="panel-grid">
      <article className="panel stack-md">
        <div className="stack-sm">
          <p className="eyebrow">App settings</p>
          <h2 className="section-title">Metadata and control surface</h2>
          <p className="page-copy">
            Inspect the immutable app record and the operational state the daemon currently exposes
            for this app.
          </p>
        </div>

        <dl className="data-grid">
          <div>
            <dt>App name</dt>
            <dd>{appName}</dd>
          </div>
          <div>
            <dt>Status</dt>
            <dd>{status}</dd>
          </div>
          <div>
            <dt>App ID</dt>
            <dd className="font-mono">{appId}</dd>
          </div>
          <div>
            <dt>Created</dt>
            <dd className="font-mono">{formatDate(createdAt)}</dd>
          </div>
          <div>
            <dt>Lock state</dt>
            <dd>{locked ? 'Locked' : 'Writable'}</dd>
          </div>
          <div>
            <dt>Lock control</dt>
            <dd>{locked ? 'Backend route missing' : 'Backend route missing'}</dd>
          </div>
        </dl>

        <p className={locked ? 'callout callout-warning' : 'callout callout-success'}>
          {locked
            ? 'This app is currently locked. Deploy and mutation routes already respect that lock state.'
            : 'This app is writable. Lock and unlock controls are still unavailable because the daemon does not expose an HTTP route for them yet.'}
        </p>
      </article>

      <article className="panel stack-md panel-danger">
        <div className="stack-sm">
          <p className="eyebrow">Danger zone</p>
          <h2 className="section-title">Delete app</h2>
          <p className="page-copy">
            This permanently removes the app record, process metadata, config, domains, and related
            dashboard-visible state for <span className="font-mono">{appName}</span>.
          </p>
        </div>

        {notice ? <p className="callout callout-success">{notice}</p> : null}
        {error ? <p className="callout callout-danger">{error}</p> : null}

        <div className="form-group">
          <label className="form-label" htmlFor="delete-app-confirm">
            Type app name to confirm
          </label>
          <input
            id="delete-app-confirm"
            className="input"
            value={confirmName}
            onChange={(event) => setConfirmName(event.target.value)}
            placeholder={appName}
            disabled={busy !== null}
          />
        </div>

        <div className="form-actions">
          <button
            type="button"
            className="btn btn-danger"
            disabled={!readyToDelete || busy !== null}
            onClick={() => setConfirmDelete(true)}
          >
            Delete app
          </button>
        </div>
      </article>

      <ConfirmModal
        open={confirmDelete}
        title={`Delete ${appName}?`}
        description="This action is permanent and removes the app from the fleet view."
        confirmLabel="Delete app"
        busy={busy === 'delete'}
        onClose={() => {
          if (busy !== 'delete') setConfirmDelete(false);
        }}
        onConfirm={() => {
          void handleDelete();
        }}
      />
    </section>
  );
}

function formatDate(value: string): string {
  return new Date(value).toLocaleString('en-US', {
    month: 'short',
    day: '2-digit',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}
