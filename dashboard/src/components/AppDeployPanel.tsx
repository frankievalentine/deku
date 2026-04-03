import { type FormEvent, useMemo, useRef, useState } from 'react';
import {
  type Deployment,
  triggerArchiveDeploy,
  triggerImageDeploy,
  triggerRollback,
} from '../lib/api';
import ConfirmModal from './ConfirmModal';
import StatusBadge from './StatusBadge';

interface AppDeployPanelProps {
  appName: string;
  locked: boolean;
  deployments: Deployment[];
  onRefresh: () => Promise<void>;
}

export default function AppDeployPanel({
  appName,
  locked,
  deployments,
  onRefresh,
}: AppDeployPanelProps) {
  const [imageRef, setImageRef] = useState('');
  const [archiveFile, setArchiveFile] = useState<File | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [rollbackTarget, setRollbackTarget] = useState<Deployment | null>(null);
  const archiveInputRef = useRef<HTMLInputElement | null>(null);

  const rollbackCandidates = useMemo(() => deployments.slice(1, 6), [deployments]);

  async function refreshAfterAction(message: string) {
    setNotice(message);
    await onRefresh();
  }

  async function handleImageDeploy(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmedImage = imageRef.trim();
    if (!trimmedImage || locked) return;

    try {
      setBusyAction('image-deploy');
      setError(null);
      setNotice(null);
      await triggerImageDeploy(appName, trimmedImage);
      setImageRef('');
      await refreshAfterAction(`Started image deploy for ${trimmedImage}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to start image deploy.');
    } finally {
      setBusyAction(null);
    }
  }

  async function handleArchiveDeploy(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!archiveFile || locked) return;

    try {
      setBusyAction('archive-deploy');
      setError(null);
      setNotice(null);
      await triggerArchiveDeploy(appName, archiveFile);
      const fileName = archiveFile.name;
      setArchiveFile(null);
      if (archiveInputRef.current) {
        archiveInputRef.current.value = '';
      }
      await refreshAfterAction(`Started archive deploy for ${fileName}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to upload archive.');
    } finally {
      setBusyAction(null);
    }
  }

  async function handleRollback() {
    if (!rollbackTarget || locked) return;

    try {
      setBusyAction(`rollback-${rollbackTarget.id}`);
      setError(null);
      setNotice(null);
      await triggerRollback(appName, rollbackTarget.id);
      const targetId = rollbackTarget.id.slice(0, 8);
      setRollbackTarget(null);
      await refreshAfterAction(`Started rollback to deployment ${targetId}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to start rollback.');
    } finally {
      setBusyAction(null);
    }
  }

  return (
    <>
      <article className="panel panel-accent stack-md">
        <div className="cluster justify-between align-start">
          <div className="stack-sm">
            <p className="eyebrow">Deploy</p>
            <h2 className="section-title">Ship and recover</h2>
            <p className="page-copy">
              Trigger image deploys, upload archive builds, and roll back to a recent deployment
              without leaving the app page.
            </p>
          </div>
          <span className="inventory-summary">{locked ? 'APP LOCKED' : 'ASYNC ACTIONS'}</span>
        </div>

        {locked ? (
          <p className="callout callout-warning">
            This app is locked. Deploy and rollback controls are disabled until it is unlocked.
          </p>
        ) : null}

        {notice ? <p className="callout callout-success">{notice}</p> : null}
        {error ? <p className="callout callout-danger">{error}</p> : null}

        <div className="deploy-grid">
          <form onSubmit={handleImageDeploy} className="deploy-card stack-md">
            <div className="stack-sm">
              <p className="eyebrow">Remote image</p>
              <h3 className="deploy-title">Deploy image reference</h3>
            </div>

            <div className="form-group">
              <label className="form-label" htmlFor="image-ref">
                OCI image
              </label>
              <input
                id="image-ref"
                className="input"
                value={imageRef}
                onChange={(event) => setImageRef(event.target.value)}
                placeholder="ghcr.io/acme/web:2026-04-02"
                disabled={locked || busyAction !== null}
              />
            </div>

            <div className="form-actions">
              <button
                className="btn btn-primary"
                type="submit"
                disabled={locked || busyAction !== null || imageRef.trim().length === 0}
              >
                {busyAction === 'image-deploy' ? 'Starting…' : 'Deploy image'}
              </button>
            </div>
          </form>

          <form onSubmit={handleArchiveDeploy} className="deploy-card stack-md">
            <div className="stack-sm">
              <p className="eyebrow">Uploaded source</p>
              <h3 className="deploy-title">Deploy archive</h3>
            </div>

            <div className="form-group">
              <label className="form-label" htmlFor="archive-input">
                Archive file
              </label>
              <input
                id="archive-input"
                ref={archiveInputRef}
                className="input file-input"
                type="file"
                onChange={(event) => setArchiveFile(event.target.files?.[0] ?? null)}
                disabled={locked || busyAction !== null}
              />
            </div>

            <p className="deploy-hint">
              {archiveFile ? `Selected: ${archiveFile.name}` : 'Choose a local archive to upload.'}
            </p>

            <div className="form-actions">
              <button
                className="btn btn-secondary"
                type="submit"
                disabled={locked || busyAction !== null || archiveFile === null}
              >
                {busyAction === 'archive-deploy' ? 'Uploading…' : 'Deploy archive'}
              </button>
            </div>
          </form>
        </div>

        <div className="stack-md">
          <div className="cluster justify-between align-center">
            <div className="stack-sm">
              <p className="eyebrow">Rollback</p>
              <h3 className="deploy-title">Recent targets</h3>
            </div>
            <span className="text-muted">Newest deployment stays in the history table below.</span>
          </div>

          {rollbackCandidates.length === 0 ? (
            <p className="text-muted">
              Rollback targets appear after the app has more than one deployment.
            </p>
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <th>ID</th>
                  <th>Status</th>
                  <th>Source</th>
                  <th>Created</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {rollbackCandidates.map((deployment) => (
                  <tr key={deployment.id}>
                    <td className="font-mono">{deployment.id.slice(0, 8)}</td>
                    <td>
                      <StatusBadge status={deployment.status} size="sm" />
                    </td>
                    <td className="font-mono">{formatSourceLabel(deployment)}</td>
                    <td className="font-mono">{formatDate(deployment.created_at)}</td>
                    <td>
                      <button
                        className="btn btn-danger btn-sm"
                        type="button"
                        onClick={() => setRollbackTarget(deployment)}
                        disabled={locked || busyAction !== null}
                      >
                        Roll back
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </article>

      <ConfirmModal
        open={rollbackTarget !== null}
        title={`Roll back ${appName}?`}
        description={
          rollbackTarget
            ? `This starts an asynchronous rollback to deployment ${rollbackTarget.id.slice(0, 8)} (${formatSourceLabel(rollbackTarget)}). Current runtime state may be replaced once the rollback completes.`
            : ''
        }
        confirmLabel="Start rollback"
        cancelLabel="Keep current"
        busy={rollbackTarget !== null && busyAction === `rollback-${rollbackTarget.id}`}
        onConfirm={() => {
          void handleRollback();
        }}
        onClose={() => setRollbackTarget(null)}
      />
    </>
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

function formatSourceLabel(deployment: Deployment): string {
  if (deployment.image_tag) {
    return truncateMiddle(deployment.image_tag, 32);
  }

  return deployment.builder;
}

function truncateMiddle(value: string, maxLength: number): string {
  if (value.length <= maxLength) return value;
  const headLength = Math.ceil((maxLength - 3) / 2);
  const tailLength = Math.floor((maxLength - 3) / 2);
  return `${value.slice(0, headLength)}...${value.slice(-tailLength)}`;
}
