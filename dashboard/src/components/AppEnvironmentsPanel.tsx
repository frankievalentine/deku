import { type SubmitEvent, useState } from 'react';
import { createEnvironment, deleteEnvironment, type Environment } from '../lib/api';
import ConfirmModal from './ConfirmModal';

interface AppEnvironmentsPanelProps {
  appName: string;
  locked: boolean;
  environments: Environment[];
  onRefresh: () => Promise<void> | void;
}

export default function AppEnvironmentsPanel({
  appName,
  locked,
  environments,
  onRefresh,
}: AppEnvironmentsPanelProps) {
  const [name, setName] = useState('');
  const [branch, setBranch] = useState('');
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [toDelete, setToDelete] = useState<Environment | null>(null);

  async function handleCreate(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (locked) return;

    const displayName = name.trim();
    if (!displayName) {
      setError('Give the environment a name, for example Staging.');
      return;
    }

    try {
      setBusy('create');
      setError(null);
      setNotice(null);
      await createEnvironment(appName, {
        name: displayName,
        branch: branch.trim() || null,
      });
      setName('');
      setBranch('');
      await onRefresh();
      setNotice(`Added ${displayName}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to add the environment.');
    } finally {
      setBusy(null);
    }
  }

  async function handleDelete() {
    if (!toDelete) return;
    const environment = toDelete;
    try {
      setBusy(`delete-${environment.slug}`);
      setError(null);
      setNotice(null);
      await deleteEnvironment(appName, environment.slug);
      setToDelete(null);
      await onRefresh();
      setNotice(`Removed ${environment.name}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to remove it.');
    } finally {
      setBusy(null);
    }
  }

  return (
    <>
      <article className="panel stack-md">
        <div className="panel-heading">
          <div className="stack-sm panel-heading-copy">
            <p className="eyebrow">Deploy targets</p>
            <h2 className="section-title">Environments</h2>
            <p className="page-copy">
              Each environment is a separate deploy target with its own config overrides. Record the
              git branch it follows for reference; pushes do not deploy on their own yet.
            </p>
          </div>
          <span className="inventory-summary">{environments.length} total</span>
        </div>

        {notice ? <p className="callout callout-success">{notice}</p> : null}
        {error ? <p className="callout callout-danger">{error}</p> : null}

        <dl className="data-grid">
          {environments.map((environment) => (
            <div key={environment.id}>
              <dt>{environment.name}</dt>
              <dd>
                {environment.is_production
                  ? 'Production - always present'
                  : `Branch: ${environment.branch || 'Not set'}`}
              </dd>
            </div>
          ))}
        </dl>

        <form onSubmit={handleCreate} className="stack-md" noValidate>
          <div className="panel-grid">
            <div className="form-group">
              <label className="form-label" htmlFor="environment-name">
                Environment name
              </label>
              <input
                id="environment-name"
                className="input"
                placeholder="Staging"
                value={name}
                onChange={(event) => setName(event.target.value)}
                disabled={locked || busy !== null}
              />
            </div>
            <div className="form-group">
              <label className="form-label" htmlFor="environment-branch">
                Git branch (optional)
              </label>
              <input
                id="environment-branch"
                className="input"
                placeholder="main"
                value={branch}
                onChange={(event) => setBranch(event.target.value)}
                disabled={locked || busy !== null}
              />
            </div>
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
              {busy === 'create' ? <span className="loading-spinner" /> : null}
              <span>Add environment</span>
            </button>
          </div>
        </form>

        {environments.some((environment) => !environment.is_production) ? (
          <div className="stack-sm">
            <h3 className="deploy-title">Remove an environment</h3>
            <div className="button-row">
              {environments
                .filter((environment) => !environment.is_production)
                .map((environment) => (
                  <button
                    key={environment.id}
                    className="btn btn-outline btn-danger-outline btn-sm"
                    type="button"
                    onClick={() => setToDelete(environment)}
                    disabled={locked || busy !== null}
                  >
                    Remove {environment.name}
                  </button>
                ))}
            </div>
          </div>
        ) : null}
      </article>

      <ConfirmModal
        open={toDelete !== null}
        title="Remove this environment?"
        description={
          toDelete
            ? `Config overrides saved for ${toDelete.name} will be removed. Deployments already made to it keep serving.`
            : ''
        }
        confirmLabel="Remove environment"
        busy={toDelete ? busy === `delete-${toDelete.slug}` : false}
        onClose={() => {
          if (!busy?.startsWith('delete-')) setToDelete(null);
        }}
        onConfirm={() => {
          void handleDelete();
        }}
      />
    </>
  );
}
