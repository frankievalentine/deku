import { type SubmitEvent, useCallback, useEffect, useState } from 'react';
import type { AppObjectStoreState } from '../lib/api';
import { fetchAppObjectStoreLink, linkAppObjectStore, unlinkAppObjectStore } from '../lib/api';
import ConfirmModal from './ConfirmModal';
import Spinner from './Spinner';

interface AppObjectStorePanelProps {
  appName: string;
  locked: boolean;
}

export default function AppObjectStorePanel({ appName, locked }: AppObjectStorePanelProps) {
  const [appLink, setAppLink] = useState<AppObjectStoreState | null>(null);
  const [prefixDraft, setPrefixDraft] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [confirmUnlink, setConfirmUnlink] = useState(false);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const nextLink = await fetchAppObjectStoreLink(appName);
      setAppLink(nextLink);
      setPrefixDraft(nextLink.link?.prefix ?? '');
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to load object store link.'
      );
    } finally {
      setLoading(false);
    }
  }, [appName]);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleLink(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (locked || !appLink?.configured) return;

    try {
      setBusy('link');
      setError(null);
      setNotice(null);
      const nextLink = await linkAppObjectStore(appName, prefixDraft.trim() || null);
      setAppLink(nextLink);
      setPrefixDraft(nextLink.link?.prefix ?? '');
      setNotice(`Object store credentials linked to ${appName}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to link credentials.');
    } finally {
      setBusy(null);
    }
  }

  async function handleUnlink() {
    try {
      setBusy('unlink');
      setError(null);
      setNotice(null);
      await unlinkAppObjectStore(appName);
      setConfirmUnlink(false);
      await load();
      setNotice(`Object store credentials removed from ${appName}.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to unlink credentials.');
    } finally {
      setBusy(null);
    }
  }

  const configured = appLink?.configured ?? false;
  const linked = appLink?.linked ?? false;

  return (
    <>
      <article className="panel stack-md">
        <div className="panel-heading">
          <div className="stack-sm panel-heading-copy">
            <p className="eyebrow">Storage</p>
            <h2 className="section-title">Object store credentials</h2>
            <p className="page-copy">
              Linking injects the saved credentials into this app as managed AWS and S3 environment
              variables.
            </p>
          </div>
          <span className="inventory-summary">{linked ? 'Linked' : 'Not linked'}</span>
        </div>

        {notice ? <p className="callout callout-success">{notice}</p> : null}
        {error ? <p className="callout callout-danger">{error}</p> : null}
        {!configured ? (
          <p className="callout callout-warning">
            No object store is configured on this host. <a href="/storage">Open storage</a> to save
            one before linking it to this app.
          </p>
        ) : !linked ? (
          <p className="text-muted">
            Set the prefix below and link the credentials to inject them into this app.
          </p>
        ) : null}

        {loading ? (
          <div className="loading-state">
            <Spinner />
            <span>Loading link status…</span>
          </div>
        ) : linked && appLink ? (
          <>
            <dl className="data-grid">
              <div>
                <dt>Provider</dt>
                <dd>{appLink.link?.provider ?? 'Unknown'}</dd>
              </div>
              <div>
                <dt>Bucket</dt>
                <dd className="font-mono">{appLink.link?.bucket ?? 'Unknown'}</dd>
              </div>
              <div>
                <dt>Region</dt>
                <dd className="font-mono">{appLink.link?.region ?? 'Unknown'}</dd>
              </div>
              <div>
                <dt>Endpoint</dt>
                <dd className="font-mono">{appLink.link?.endpoint ?? 'Unknown'}</dd>
              </div>
              <div>
                <dt>Prefix</dt>
                <dd className="font-mono">{appLink.link?.prefix || 'None'}</dd>
              </div>
              <div>
                <dt>Path style</dt>
                <dd>{appLink.link?.path_style ? 'Enabled' : 'Disabled'}</dd>
              </div>
            </dl>

            <div className="stack-sm">
              <h3 className="deploy-title">Managed environment keys</h3>
              {appLink.link?.linked_keys.length ? (
                <dl className="data-grid">
                  {appLink.link.linked_keys.map((key) => (
                    <div key={key}>
                      <dt>{key}</dt>
                      <dd>Linked</dd>
                    </div>
                  ))}
                </dl>
              ) : (
                <p className="text-muted">No managed keys are set for this app.</p>
              )}
            </div>
          </>
        ) : null}

        <form onSubmit={handleLink} className="stack-md" noValidate>
          <div className="form-group">
            <label className="form-label" htmlFor="app-objectstore-prefix">
              Prefix override
            </label>
            <input
              id="app-objectstore-prefix"
              className="input"
              value={prefixDraft}
              onChange={(event) => setPrefixDraft(event.target.value)}
              placeholder="apps/my-app/"
              disabled={locked || busy !== null || !configured}
            />
          </div>
          <div className="form-actions">
            <button
              className="btn btn-primary"
              type="submit"
              disabled={locked || busy !== null || !configured}
            >
              {busy === 'link' ? <span className="loading-spinner" /> : null}
              <span>Link app</span>
            </button>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => {
                void load();
              }}
              disabled={busy !== null}
            >
              Refresh status
            </button>
            <button
              type="button"
              className="btn btn-danger"
              onClick={() => setConfirmUnlink(true)}
              disabled={locked || busy !== null || !linked}
            >
              Unlink app
            </button>
          </div>
        </form>
      </article>

      <ConfirmModal
        open={confirmUnlink}
        title="Remove app object store link?"
        description={`This will delete the managed AWS and S3 object store variables from ${appName}.`}
        confirmLabel="Unlink app"
        busy={busy === 'unlink'}
        onClose={() => {
          if (busy !== 'unlink') setConfirmUnlink(false);
        }}
        onConfirm={() => {
          void handleUnlink();
        }}
      />
    </>
  );
}
