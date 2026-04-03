import {
  type Dispatch,
  type FormEvent,
  type SetStateAction,
  useCallback,
  useEffect,
  useState,
} from 'react';
import type { ObjectStoreConfig, ObjectStoreState } from '../lib/api';
import {
  fetchObjectStoreConfig,
  getToken,
  setObjectStoreConfig,
  testObjectStoreConfig,
  unsetObjectStoreConfig,
} from '../lib/api';
import ConfirmModal from './ConfirmModal';
import ConnectScreen from './ConnectScreen';

interface ObjectStoreDraft {
  provider: string;
  bucket: string;
  region: string;
  endpoint: string;
  access_key_id: string;
  secret_access_key: string;
  path_style: boolean;
  prefix: string;
}

const EMPTY_DRAFT: ObjectStoreDraft = {
  provider: 'r2',
  bucket: '',
  region: 'auto',
  endpoint: '',
  access_key_id: '',
  secret_access_key: '',
  path_style: true,
  prefix: '',
};

export default function ObjectStorePage() {
  const [hasToken, setHasToken] = useState(() => Boolean(getToken()));

  if (!hasToken) {
    return <ConnectScreen onConnected={() => setHasToken(true)} />;
  }

  return <ObjectStoreInner />;
}

function ObjectStoreInner() {
  const [state, setState] = useState<ObjectStoreState | null>(null);
  const [draft, setDraft] = useState<ObjectStoreDraft>(EMPTY_DRAFT);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [confirmUnset, setConfirmUnset] = useState(false);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const nextState = await fetchObjectStoreConfig();
      setState(nextState);
      setDraft(buildDraft(nextState.object_store));
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to load object store config.'
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const payload = buildPayload(draft);

    if (!payload) {
      setError('Provider, bucket, region, endpoint, access key ID, and secret are required.');
      return;
    }

    try {
      setBusy('save');
      setError(null);
      setNotice(null);
      await setObjectStoreConfig(payload);
      await load();
      setNotice(
        `Saved ${payload.provider} object store configuration for bucket ${payload.bucket}.`
      );
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to save object store config.'
      );
    } finally {
      setBusy(null);
    }
  }

  async function handleTest() {
    try {
      setBusy('test');
      setError(null);
      setNotice(null);
      await testObjectStoreConfig();
      setNotice('Stored object store configuration passed the connectivity check.');
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to test object store config.'
      );
    } finally {
      setBusy(null);
    }
  }

  async function handleUnset() {
    try {
      setBusy('unset');
      setError(null);
      setNotice(null);
      await unsetObjectStoreConfig();
      setConfirmUnset(false);
      await load();
      setNotice('Object store configuration removed.');
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to remove object store config.'
      );
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <span className="loading-spinner" />
        <span>Loading object store settings…</span>
      </div>
    );
  }

  if (!state) {
    return (
      <div className="panel error-state">
        Failed to load object store settings: {error ?? 'unknown error'}
      </div>
    );
  }

  const config = state.object_store;

  return (
    <>
      <div className="stack-lg">
        <section className="hero-panel">
          <div className="stack-md">
            <p className="eyebrow">Host storage</p>
            <h1 className="page-title">Object store configuration</h1>
            <p className="page-copy">
              Configure the S3-compatible store used for managed Postgres backups and other host
              features that need durable object storage.
            </p>
          </div>
          <div className="metrics-grid">
            <Metric label="Configured" value={state.configured ? 'Yes' : 'No'} />
            <Metric label="Provider" value={config?.provider ?? 'Unset'} />
            <Metric label="Bucket" value={config?.bucket ?? 'Unset'} />
            <Metric
              label="Addressing"
              value={config ? (config.path_style ? 'Path' : 'Virtual') : 'Unset'}
            />
          </div>
        </section>

        {notice ? <p className="callout callout-success">{notice}</p> : null}
        {error ? <p className="callout callout-danger">{error}</p> : null}

        <div className="panel-grid">
          <article className="panel stack-md">
            <div className="stack-sm">
              <p className="eyebrow">Configuration</p>
              <h2 className="section-title">Connection details</h2>
              <p className="page-copy">
                The backend stores a redacted copy of the secret access key, so the dashboard must
                send a fresh secret on every save.
              </p>
            </div>

            <form onSubmit={handleSave} className="stack-md">
              <div className="panel-grid">
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-provider">
                    Provider
                  </label>
                  <input
                    id="objectstore-provider"
                    className="input"
                    value={draft.provider}
                    onChange={(event) => updateDraft(setDraft, 'provider', event.target.value)}
                    placeholder="r2"
                    disabled={busy !== null}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-bucket">
                    Bucket
                  </label>
                  <input
                    id="objectstore-bucket"
                    className="input"
                    value={draft.bucket}
                    onChange={(event) => updateDraft(setDraft, 'bucket', event.target.value)}
                    placeholder="deku"
                    disabled={busy !== null}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-region">
                    Region
                  </label>
                  <input
                    id="objectstore-region"
                    className="input"
                    value={draft.region}
                    onChange={(event) => updateDraft(setDraft, 'region', event.target.value)}
                    placeholder="auto"
                    disabled={busy !== null}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-endpoint">
                    Endpoint
                  </label>
                  <input
                    id="objectstore-endpoint"
                    className="input"
                    value={draft.endpoint}
                    onChange={(event) => updateDraft(setDraft, 'endpoint', event.target.value)}
                    placeholder="https://<account>.r2.cloudflarestorage.com"
                    disabled={busy !== null}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-access-key">
                    Access key ID
                  </label>
                  <input
                    id="objectstore-access-key"
                    className="input"
                    value={draft.access_key_id}
                    onChange={(event) => updateDraft(setDraft, 'access_key_id', event.target.value)}
                    placeholder="AKIA..."
                    disabled={busy !== null}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-secret-key">
                    Secret access key
                  </label>
                  <input
                    id="objectstore-secret-key"
                    className="input"
                    type="password"
                    value={draft.secret_access_key}
                    onChange={(event) =>
                      updateDraft(setDraft, 'secret_access_key', event.target.value)
                    }
                    placeholder={state.configured ? 'Re-enter current secret to save changes' : ''}
                    disabled={busy !== null}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-prefix">
                    Prefix
                  </label>
                  <input
                    id="objectstore-prefix"
                    className="input"
                    value={draft.prefix}
                    onChange={(event) => updateDraft(setDraft, 'prefix', event.target.value)}
                    placeholder="backups/deku"
                    disabled={busy !== null}
                  />
                </div>
              </div>

              <label className="checkbox-field" htmlFor="objectstore-path-style">
                <input
                  id="objectstore-path-style"
                  className="checkbox-input"
                  type="checkbox"
                  checked={draft.path_style}
                  onChange={(event) =>
                    setDraft((current) => ({ ...current, path_style: event.target.checked }))
                  }
                  disabled={busy !== null}
                />
                <span>
                  Use path-style addressing
                  <small>Disable this for virtual-host-style compatible providers.</small>
                </span>
              </label>

              <div className="form-actions">
                <button className="btn btn-primary" type="submit" disabled={busy !== null}>
                  {busy === 'save' ? 'Saving…' : 'Save configuration'}
                </button>
              </div>
            </form>
          </article>

          <article className="panel stack-md">
            <div className="stack-sm">
              <p className="eyebrow">Stored config</p>
              <h2 className="section-title">Status and actions</h2>
            </div>

            {state.configured && config ? (
              <>
                <dl className="data-grid">
                  <div>
                    <dt>Provider</dt>
                    <dd>{config.provider}</dd>
                  </div>
                  <div>
                    <dt>Bucket</dt>
                    <dd className="font-mono">{config.bucket}</dd>
                  </div>
                  <div>
                    <dt>Region</dt>
                    <dd className="font-mono">{config.region}</dd>
                  </div>
                  <div>
                    <dt>Endpoint</dt>
                    <dd className="font-mono">{config.endpoint}</dd>
                  </div>
                  <div>
                    <dt>Access key ID</dt>
                    <dd className="font-mono">{config.access_key_id}</dd>
                  </div>
                  <div>
                    <dt>Secret</dt>
                    <dd className="font-mono">{config.secret_access_key || 'Not stored'}</dd>
                  </div>
                  <div>
                    <dt>Path style</dt>
                    <dd>{config.path_style ? 'Enabled' : 'Disabled'}</dd>
                  </div>
                  <div>
                    <dt>Prefix</dt>
                    <dd className="font-mono">{config.prefix ?? 'None'}</dd>
                  </div>
                </dl>

                <p className="callout callout-warning">
                  Connectivity tests run against the saved host configuration, not the unsaved form
                  draft.
                </p>

                <div className="form-actions">
                  <button
                    type="button"
                    className="btn btn-secondary"
                    onClick={() => {
                      void handleTest();
                    }}
                    disabled={busy !== null}
                  >
                    {busy === 'test' ? 'Testing…' : 'Test configuration'}
                  </button>
                  <button
                    type="button"
                    className="btn btn-danger"
                    onClick={() => setConfirmUnset(true)}
                    disabled={busy !== null}
                  >
                    Remove configuration
                  </button>
                </div>
              </>
            ) : (
              <>
                <p className="text-muted">
                  No object store has been configured yet. Save a provider, bucket, endpoint, and
                  credentials to enable backup storage.
                </p>
                <div className="form-actions">
                  <button type="button" className="btn btn-secondary" disabled>
                    Test configuration
                  </button>
                  <button type="button" className="btn btn-danger" disabled>
                    Remove configuration
                  </button>
                </div>
              </>
            )}
          </article>
        </div>
      </div>

      <ConfirmModal
        open={confirmUnset}
        title="Remove object store configuration?"
        description="Managed backup restores will stop working until a new object store config is saved."
        confirmLabel="Remove config"
        busy={busy === 'unset'}
        onClose={() => {
          if (busy !== 'unset') setConfirmUnset(false);
        }}
        onConfirm={() => {
          void handleUnset();
        }}
      />
    </>
  );
}

function buildDraft(config: ObjectStoreConfig | null): ObjectStoreDraft {
  if (!config) {
    return { ...EMPTY_DRAFT };
  }

  return {
    provider: config.provider,
    bucket: config.bucket,
    region: config.region,
    endpoint: config.endpoint,
    access_key_id: config.access_key_id,
    secret_access_key: '',
    path_style: config.path_style,
    prefix: config.prefix ?? '',
  };
}

function buildPayload(draft: ObjectStoreDraft): ObjectStoreConfig | null {
  const provider = draft.provider.trim();
  const bucket = draft.bucket.trim();
  const region = draft.region.trim();
  const endpoint = draft.endpoint.trim();
  const accessKeyId = draft.access_key_id.trim();
  const secretAccessKey = draft.secret_access_key.trim();
  const prefix = draft.prefix.trim();

  if (!provider || !bucket || !region || !endpoint || !accessKeyId || !secretAccessKey) {
    return null;
  }

  return {
    provider,
    bucket,
    region,
    endpoint,
    access_key_id: accessKeyId,
    secret_access_key: secretAccessKey,
    path_style: draft.path_style,
    prefix: prefix || null,
  };
}

function updateDraft<K extends keyof ObjectStoreDraft>(
  setDraft: Dispatch<SetStateAction<ObjectStoreDraft>>,
  key: K,
  value: ObjectStoreDraft[K]
) {
  setDraft((current) => ({ ...current, [key]: value }));
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric-card">
      <p>{label}</p>
      <strong>{value}</strong>
    </div>
  );
}
