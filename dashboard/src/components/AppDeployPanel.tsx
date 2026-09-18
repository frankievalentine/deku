import { type SubmitEvent, useMemo, useRef, useState } from 'react';
import {
  type Deployment,
  type Environment,
  triggerArchiveDeploy,
  triggerImageDeploy,
  triggerRollback,
} from '../lib/api';
import ConfirmModal from './ConfirmModal';
import StatusBadge from './StatusBadge';
import TableScroll from './TableScroll';

/** Names the environment in a notice, or nothing when deploying to production. */
function deployTargetSuffix(environment: string): string {
  return environment ? ` into ${environment}` : '';
}

interface AppDeployPanelProps {
  appName: string;
  locked: boolean;
  deployments: Deployment[];
  environments: Environment[];
  /** Slug to deploy into; empty targets production. */
  environment: string;
  onEnvironmentChange: (value: string) => void;
  onRefresh: () => Promise<void>;
}

export default function AppDeployPanel({
  appName,
  locked,
  deployments,
  environments,
  environment,
  onEnvironmentChange,
  onRefresh,
}: AppDeployPanelProps) {
  const [imageRef, setImageRef] = useState('');
  const [archiveFile, setArchiveFile] = useState<File | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [imageError, setImageError] = useState<string | null>(null);
  const [archiveError, setArchiveError] = useState<string | null>(null);
  const [rollbackTarget, setRollbackTarget] = useState<Deployment | null>(null);
  const archiveInputRef = useRef<HTMLInputElement | null>(null);
  const imageInputRef = useRef<HTMLInputElement | null>(null);

  const rollbackCandidates = useMemo(() => deployments.slice(1, 6), [deployments]);

  async function refreshAfterAction(message: string) {
    setNotice(message);
    await onRefresh();
  }

  async function handleImageDeploy(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmedImage = imageRef.trim();
    if (locked) return;

    if (!trimmedImage) {
      setImageError('Enter a container image reference, for example ghcr.io/acme/web:latest.');
      imageInputRef.current?.focus();
      return;
    }

    setImageError(null);

    try {
      setBusyAction('image-deploy');
      setError(null);
      setNotice(null);
      await triggerImageDeploy(appName, trimmedImage, environment || undefined);
      setImageRef('');
      await refreshAfterAction(
        `Started image deploy for ${trimmedImage}${deployTargetSuffix(environment)}.`
      );
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to start image deploy.');
    } finally {
      setBusyAction(null);
    }
  }

  async function handleArchiveDeploy(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (locked) return;

    if (!archiveFile) {
      setArchiveError('Choose an archive file to deploy.');
      archiveInputRef.current?.focus();
      return;
    }

    setArchiveError(null);

    try {
      setBusyAction('archive-deploy');
      setError(null);
      setNotice(null);
      await triggerArchiveDeploy(appName, archiveFile, environment || undefined);
      const fileName = archiveFile.name;
      setArchiveFile(null);
      if (archiveInputRef.current) {
        archiveInputRef.current.value = '';
      }
      await refreshAfterAction(
        `Started archive deploy for ${fileName}${deployTargetSuffix(environment)}.`
      );
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
        <div className="panel-heading">
          <div className="stack-sm panel-heading-copy">
            <p className="eyebrow">Deploy</p>
            <h2 className="section-title">Deploy and roll back</h2>
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

        <div className="form-group">
          <label className="form-label" htmlFor="deploy-environment">
            Deploy into
          </label>
          <select
            id="deploy-environment"
            className="input"
            value={environment}
            onChange={(event) => onEnvironmentChange(event.target.value)}
            disabled={locked || busyAction !== null}
          >
            <option value="">production</option>
            {environments
              .filter((entry) => !entry.is_production)
              .map((entry) => (
                <option key={entry.id} value={entry.slug}>
                  {entry.name} ({entry.slug})
                </option>
              ))}
          </select>
        </div>

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
                ref={imageInputRef}
                className="input"
                value={imageRef}
                onChange={(event) => {
                  setImageRef(event.target.value);
                  if (imageError) setImageError(null);
                }}
                placeholder="ghcr.io/acme/web:2026-04-02"
                aria-invalid={imageError ? true : undefined}
                aria-describedby={imageError ? 'image-ref-error' : undefined}
                autoComplete="off"
                spellCheck={false}
                disabled={locked || busyAction !== null}
              />
              {imageError ? (
                <p id="image-ref-error" className="form-error">
                  {imageError}
                </p>
              ) : null}
            </div>

            <div className="form-actions">
              <button
                className="btn btn-primary"
                type="submit"
                disabled={locked || busyAction !== null}
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
                className="file-input-native"
                type="file"
                aria-invalid={archiveError ? true : undefined}
                aria-describedby={archiveError ? 'archive-input-error' : undefined}
                onChange={(event) => {
                  setArchiveFile(event.target.files?.[0] ?? null);
                  if (archiveError) setArchiveError(null);
                }}
                disabled={locked || busyAction !== null}
              />
              <label
                className={`file-picker${locked || busyAction !== null ? ' is-disabled' : ''}`}
                htmlFor="archive-input"
              >
                <span className="file-picker-button">Choose file</span>
                <span className="file-picker-value">
                  {archiveFile ? archiveFile.name : 'No file chosen'}
                </span>
              </label>
            </div>

            <p className="deploy-hint">
              {archiveFile ? `Selected: ${archiveFile.name}` : 'Choose a local archive to upload.'}
            </p>
            {archiveError ? (
              <p id="archive-input-error" className="form-error">
                {archiveError}
              </p>
            ) : null}

            <div className="form-actions">
              <button
                className="btn btn-secondary"
                type="submit"
                disabled={locked || busyAction !== null}
              >
                {busyAction === 'archive-deploy' ? 'Uploading…' : 'Deploy archive'}
              </button>
            </div>
          </form>
        </div>

        <div className="stack-md">
          <div className="panel-heading panel-heading-top">
            <div className="stack-sm panel-heading-copy">
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
            <TableScroll>
              <table className="table">
                <caption className="sr-only">Rollback targets for this app</caption>
                <thead>
                  <tr>
                    <th scope="col">ID</th>
                    <th scope="col">Status</th>
                    <th scope="col">Source</th>
                    <th scope="col">Created</th>
                    <th scope="col">
                      <span className="sr-only">Actions</span>
                    </th>
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
                          className="btn btn-outline btn-danger-outline btn-sm"
                          type="button"
                          aria-label={`Roll back to deployment ${deployment.id.slice(0, 8)}`}
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
            </TableScroll>
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
