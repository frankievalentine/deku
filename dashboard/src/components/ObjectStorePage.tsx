import {
  type Dispatch,
  type SetStateAction,
  type SubmitEvent,
  useCallback,
  useEffect,
  useState,
} from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import type { App, AppObjectStoreState, ObjectStoreConfig, ObjectStoreState } from '../lib/api';
import {
  fetchAppObjectStoreLink,
  fetchApps,
  fetchObjectStoreConfig,
  linkAppObjectStore,
  setObjectStoreConfig,
  testObjectStoreConfig,
  unlinkAppObjectStore,
  unsetObjectStoreConfig,
} from '../lib/api';
import ConfirmModal from './ConfirmModal';
import ConnectScreen from './ConnectScreen';
import Spinner from './Spinner';

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

interface ObjectStoreFieldErrors {
  provider?: string;
  bucket?: string;
  region?: string;
  endpoint?: string;
  access_key_id?: string;
  secret_access_key?: string;
}

const REQUIRED_DRAFT_FIELDS: ReadonlyArray<{
  key: keyof ObjectStoreFieldErrors;
  inputId: string;
}> = [
  { key: 'provider', inputId: 'objectstore-provider' },
  { key: 'bucket', inputId: 'objectstore-bucket' },
  { key: 'region', inputId: 'objectstore-region' },
  { key: 'endpoint', inputId: 'objectstore-endpoint' },
  { key: 'access_key_id', inputId: 'objectstore-access-key' },
  { key: 'secret_access_key', inputId: 'objectstore-secret-key' },
];

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
  const tokenAccess = useTokenAccess();

  if (tokenAccess === 'unknown') {
    return null;
  }

  if (tokenAccess === 'locked') {
    return <ConnectScreen />;
  }

  return <ObjectStoreInner />;
}

function ObjectStoreInner() {
  const [apps, setApps] = useState<App[]>([]);
  const [state, setState] = useState<ObjectStoreState | null>(null);
  const [selectedApp, setSelectedApp] = useState('');
  const [appLink, setAppLink] = useState<AppObjectStoreState | null>(null);
  const [appPrefixDraft, setAppPrefixDraft] = useState('');
  const [draft, setDraft] = useState<ObjectStoreDraft>(EMPTY_DRAFT);
  const [loading, setLoading] = useState(true);
  const [linkLoading, setLinkLoading] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<ObjectStoreFieldErrors>({});
  const [linkError, setLinkError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [confirmUnset, setConfirmUnset] = useState(false);
  const [confirmUnlink, setConfirmUnlink] = useState(false);

  function updateField<K extends keyof ObjectStoreDraft>(key: K, value: ObjectStoreDraft[K]) {
    updateDraft(setDraft, key, value);

    if (!isRequiredDraftField(key)) {
      return;
    }

    setFieldErrors((current) => {
      const next: ObjectStoreFieldErrors = { ...current };
      delete next[key];
      return next;
    });
  }

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setLoadError(null);
      const [nextState, nextApps] = await Promise.all([fetchObjectStoreConfig(), fetchApps()]);
      setState(nextState);
      setApps(nextApps);
      setDraft(buildDraft(nextState.object_store));
      setSelectedApp((current) => chooseSelectedApp(current, nextApps));
    } catch (nextError) {
      setLoadError(
        nextError instanceof Error ? nextError.message : 'Unable to load object store config.'
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const loadAppLink = useCallback(async (appName: string) => {
    if (!appName) {
      setAppLink(null);
      setAppPrefixDraft('');
      return;
    }

    try {
      setLinkLoading(true);
      setActionError(null);
      const nextLink = await fetchAppObjectStoreLink(appName);
      setAppLink(nextLink);
      setAppPrefixDraft(nextLink.link?.prefix ?? '');
    } catch (nextError) {
      setActionError(
        nextError instanceof Error ? nextError.message : 'Unable to load app object store link.'
      );
    } finally {
      setLinkLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadAppLink(selectedApp);
  }, [loadAppLink, selectedApp]);

  async function handleSave(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const payload = buildPayload(draft);

    if (!payload) {
      const nextErrors = validateDraft(draft);
      setFieldErrors(nextErrors);
      const firstMissing = REQUIRED_DRAFT_FIELDS.find((field) => nextErrors[field.key]);
      if (firstMissing) {
        document.getElementById(firstMissing.inputId)?.focus();
      }
      return;
    }

    try {
      setBusy('save');
      setActionError(null);
      setFieldErrors({});
      setNotice(null);
      await setObjectStoreConfig(payload);
      await load();
      setNotice(
        `Saved ${payload.provider} object store configuration for bucket ${payload.bucket}.`
      );
    } catch (nextError) {
      setActionError(
        nextError instanceof Error ? nextError.message : 'Unable to save object store config.'
      );
    } finally {
      setBusy(null);
    }
  }

  async function handleTest() {
    try {
      setBusy('test');
      setActionError(null);
      setNotice(null);
      await testObjectStoreConfig();
      setNotice('Stored object store configuration passed the connectivity check.');
    } catch (nextError) {
      setActionError(
        nextError instanceof Error ? nextError.message : 'Unable to test object store config.'
      );
    } finally {
      setBusy(null);
    }
  }

  async function handleUnset() {
    try {
      setBusy('unset');
      setActionError(null);
      setNotice(null);
      await unsetObjectStoreConfig();
      setConfirmUnset(false);
      await load();
      setNotice('Object store configuration removed.');
    } catch (nextError) {
      setActionError(
        nextError instanceof Error ? nextError.message : 'Unable to remove object store config.'
      );
    } finally {
      setBusy(null);
    }
  }

  async function handleLink(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedApp) {
      setLinkError('Select an app to link.');
      document.getElementById('objectstore-app')?.focus();
      return;
    }

    try {
      setBusy(`link-${selectedApp}`);
      setActionError(null);
      setLinkError(null);
      setNotice(null);
      const nextLink = await linkAppObjectStore(selectedApp, appPrefixDraft.trim() || null);
      setAppLink(nextLink);
      setAppPrefixDraft(nextLink.link?.prefix ?? '');
      setNotice(`Object store credentials linked to ${selectedApp}.`);
    } catch (nextError) {
      setActionError(nextError instanceof Error ? nextError.message : 'Unable to link app.');
    } finally {
      setBusy(null);
    }
  }

  async function handleUnlink() {
    if (!selectedApp) {
      return;
    }

    try {
      setBusy(`unlink-${selectedApp}`);
      setActionError(null);
      setNotice(null);
      await unlinkAppObjectStore(selectedApp);
      setConfirmUnlink(false);
      await loadAppLink(selectedApp);
      setNotice(`Object store credentials removed from ${selectedApp}.`);
    } catch (nextError) {
      setActionError(nextError instanceof Error ? nextError.message : 'Unable to unlink app.');
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <Spinner />
        <span>Loading object store configuration…</span>
      </div>
    );
  }

  if (!state) {
    return (
      <div className="panel error-state">
        <p className="text-danger" role="alert">
          {loadError ?? 'Unable to load object store configuration.'}
        </p>
        <button type="button" className="btn btn-secondary" onClick={() => void load()}>
          Retry loading configuration
        </button>
      </div>
    );
  }

  const config = state.object_store;

  return (
    <>
      <div className="stack-lg">
        <section className="hero-panel">
          <div className="stack-md">
            <h1 className="page-title">Object store</h1>
            <p className="page-copy">
              The S3-compatible store used for managed backups and app credentials.
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

        {notice ? (
          <p className="callout callout-success" role="status">
            {notice}
          </p>
        ) : null}
        {(actionError ?? loadError) ? (
          <p className="callout callout-danger" role="alert">
            {actionError ?? loadError}
          </p>
        ) : null}

        <div className="panel-grid object-store-layout">
          <article className="panel stack-md panel-compact">
            <h2 className="section-title">Connection details</h2>
            <p className="text-muted">
              Only a redacted copy of the secret access key is kept, so enter the secret again on
              every save.
            </p>

            <form onSubmit={handleSave} className="stack-md" noValidate>
              <div className="panel-grid">
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-provider">
                    Provider
                  </label>
                  <input
                    id="objectstore-provider"
                    className="input"
                    value={draft.provider}
                    onChange={(event) => updateField('provider', event.target.value)}
                    placeholder="r2"
                    disabled={busy !== null}
                    autoComplete="off"
                    required
                    aria-invalid={fieldErrors.provider ? true : undefined}
                    aria-describedby={fieldErrors.provider ? fieldErrorId('provider') : undefined}
                  />
                  {fieldErrors.provider ? (
                    <p id={fieldErrorId('provider')} className="text-danger">
                      {fieldErrors.provider}
                    </p>
                  ) : null}
                </div>
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-bucket">
                    Bucket
                  </label>
                  <input
                    id="objectstore-bucket"
                    className="input"
                    value={draft.bucket}
                    onChange={(event) => updateField('bucket', event.target.value)}
                    placeholder="deku"
                    disabled={busy !== null}
                    autoComplete="off"
                    required
                    aria-invalid={fieldErrors.bucket ? true : undefined}
                    aria-describedby={fieldErrors.bucket ? fieldErrorId('bucket') : undefined}
                  />
                  {fieldErrors.bucket ? (
                    <p id={fieldErrorId('bucket')} className="text-danger">
                      {fieldErrors.bucket}
                    </p>
                  ) : null}
                </div>
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-region">
                    Region
                  </label>
                  <input
                    id="objectstore-region"
                    className="input"
                    value={draft.region}
                    onChange={(event) => updateField('region', event.target.value)}
                    placeholder="auto"
                    disabled={busy !== null}
                    autoComplete="off"
                    required
                    aria-invalid={fieldErrors.region ? true : undefined}
                    aria-describedby={fieldErrors.region ? fieldErrorId('region') : undefined}
                  />
                  {fieldErrors.region ? (
                    <p id={fieldErrorId('region')} className="text-danger">
                      {fieldErrors.region}
                    </p>
                  ) : null}
                </div>
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-endpoint">
                    Endpoint
                  </label>
                  <input
                    id="objectstore-endpoint"
                    className="input"
                    value={draft.endpoint}
                    onChange={(event) => updateField('endpoint', event.target.value)}
                    placeholder="https://<account>.r2.cloudflarestorage.com"
                    disabled={busy !== null}
                    autoComplete="off"
                    required
                    aria-invalid={fieldErrors.endpoint ? true : undefined}
                    aria-describedby={fieldErrors.endpoint ? fieldErrorId('endpoint') : undefined}
                  />
                  {fieldErrors.endpoint ? (
                    <p id={fieldErrorId('endpoint')} className="text-danger">
                      {fieldErrors.endpoint}
                    </p>
                  ) : null}
                </div>
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-access-key">
                    Access key ID
                  </label>
                  <input
                    id="objectstore-access-key"
                    className="input"
                    value={draft.access_key_id}
                    onChange={(event) => updateField('access_key_id', event.target.value)}
                    placeholder="AKIA..."
                    disabled={busy !== null}
                    autoComplete="off"
                    spellCheck={false}
                    required
                    aria-invalid={fieldErrors.access_key_id ? true : undefined}
                    aria-describedby={
                      fieldErrors.access_key_id ? fieldErrorId('access_key_id') : undefined
                    }
                  />
                  {fieldErrors.access_key_id ? (
                    <p id={fieldErrorId('access_key_id')} className="text-danger">
                      {fieldErrors.access_key_id}
                    </p>
                  ) : null}
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
                    onChange={(event) => updateField('secret_access_key', event.target.value)}
                    placeholder={state.configured ? 'Re-enter current secret to save changes' : ''}
                    disabled={busy !== null}
                    autoComplete="new-password"
                    required
                    aria-invalid={fieldErrors.secret_access_key ? true : undefined}
                    aria-describedby={
                      fieldErrors.secret_access_key ? fieldErrorId('secret_access_key') : undefined
                    }
                  />
                  {fieldErrors.secret_access_key ? (
                    <p id={fieldErrorId('secret_access_key')} className="text-danger">
                      {fieldErrors.secret_access_key}
                    </p>
                  ) : null}
                </div>
                <div className="form-group">
                  <label className="form-label" htmlFor="objectstore-prefix">
                    Prefix
                  </label>
                  <input
                    id="objectstore-prefix"
                    className="input"
                    value={draft.prefix}
                    onChange={(event) => updateField('prefix', event.target.value)}
                    placeholder="backups/deku"
                    disabled={busy !== null}
                    autoComplete="off"
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
                  {busy === 'save' ? <span className="loading-spinner" /> : null}
                  <span>Save configuration</span>
                </button>
              </div>
            </form>
          </article>

          <article className="panel summary-card summary-card-fit summary-card-roomy">
            <div className="summary-card-body">
              <h2 className="section-title">Saved configuration</h2>

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
                    Connectivity tests run against the saved host configuration, not the unsaved
                    form draft.
                  </p>
                </>
              ) : (
                <p className="text-muted">
                  No object store is configured. Fill in Connection details and save to enable
                  managed backups.
                </p>
              )}
            </div>

            <div className="form-actions summary-card-actions">
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => {
                  void handleTest();
                }}
                disabled={!state.configured || busy !== null}
              >
                {busy === 'test' ? <span className="loading-spinner" /> : null}
                <span>Test configuration</span>
              </button>
              <button
                type="button"
                className="btn btn-danger"
                onClick={() => setConfirmUnset(true)}
                disabled={!state.configured || busy !== null}
              >
                Remove configuration
              </button>
            </div>
          </article>

          <article className="panel stack-md">
            <h2 className="section-title">App credentials</h2>
            <p className="text-muted">
              Linking injects the saved credentials into an app as managed AWS and S3 environment
              variables.
            </p>

            {apps.length === 0 ? (
              <p className="text-muted">
                No apps exist yet. Create an app to link these credentials to it.
              </p>
            ) : (
              <>
                {state.configured ? null : (
                  <p className="callout callout-warning" id="objectstore-link-hint">
                    Save an object store configuration before linking it to an app.
                  </p>
                )}
                <div className="panel-grid">
                  <div className="form-group">
                    <label className="form-label" htmlFor="objectstore-app">
                      App
                    </label>
                    <select
                      id="objectstore-app"
                      className="input"
                      value={selectedApp}
                      onChange={(event) => {
                        setSelectedApp(event.target.value);
                        if (linkError) setLinkError(null);
                      }}
                      disabled={busy !== null}
                      aria-invalid={linkError ? true : undefined}
                      aria-describedby={linkError ? 'objectstore-app-error' : undefined}
                    >
                      {apps.map((app) => (
                        <option key={app.id} value={app.name}>
                          {app.name}
                        </option>
                      ))}
                    </select>
                    {linkError ? (
                      <p id="objectstore-app-error" className="text-danger">
                        {linkError}
                      </p>
                    ) : null}
                  </div>
                  <div className="form-group">
                    <label className="form-label" htmlFor="objectstore-app-prefix">
                      Prefix override
                    </label>
                    <input
                      id="objectstore-app-prefix"
                      className="input"
                      value={appPrefixDraft}
                      onChange={(event) => setAppPrefixDraft(event.target.value)}
                      placeholder="apps/my-app/"
                      disabled={busy !== null || !state.configured}
                    />
                  </div>
                </div>

                {linkLoading ? (
                  <div className="panel loading-state">
                    <Spinner />
                    <span>Loading app link status…</span>
                  </div>
                ) : appLink ? (
                  <>
                    <dl className="data-grid">
                      <div>
                        <dt>Linked</dt>
                        <dd>{appLink.linked ? 'Yes' : 'No'}</dd>
                      </div>
                      <div>
                        <dt>Provider</dt>
                        <dd>{appLink.link?.provider ?? 'Unavailable'}</dd>
                      </div>
                      <div>
                        <dt>Bucket</dt>
                        <dd className="font-mono">{appLink.link?.bucket ?? 'Unavailable'}</dd>
                      </div>
                      <div>
                        <dt>Region</dt>
                        <dd className="font-mono">{appLink.link?.region ?? 'Unavailable'}</dd>
                      </div>
                      <div>
                        <dt>Endpoint</dt>
                        <dd className="font-mono">{appLink.link?.endpoint ?? 'Unavailable'}</dd>
                      </div>
                      <div>
                        <dt>Prefix</dt>
                        <dd className="font-mono">{appLink.link?.prefix ?? 'Unavailable'}</dd>
                      </div>
                      <div>
                        <dt>Path style</dt>
                        <dd>
                          {appLink.link
                            ? appLink.link.path_style
                              ? 'Enabled'
                              : 'Disabled'
                            : 'Unavailable'}
                        </dd>
                      </div>
                      <div>
                        <dt>Secret present</dt>
                        <dd>
                          {appLink.link
                            ? appLink.link.secret_present
                              ? 'Yes'
                              : 'No'
                            : 'Unavailable'}
                        </dd>
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
                        <p className="text-muted">
                          No managed keys are set for this app yet. Link the credentials to add
                          them.
                        </p>
                      )}
                    </div>
                  </>
                ) : null}

                <form onSubmit={handleLink} className="form-actions">
                  <button
                    className="btn btn-primary"
                    type="submit"
                    disabled={busy !== null || !state.configured}
                    aria-describedby={state.configured ? undefined : 'objectstore-link-hint'}
                  >
                    {busy === `link-${selectedApp}` ? <span className="loading-spinner" /> : null}
                    <span>Link app</span>
                  </button>
                  <button
                    type="button"
                    className="btn btn-secondary"
                    onClick={() => {
                      void loadAppLink(selectedApp);
                    }}
                    disabled={busy !== null || !selectedApp}
                  >
                    Refresh status
                  </button>
                  <button
                    type="button"
                    className="btn btn-danger"
                    onClick={() => setConfirmUnlink(true)}
                    disabled={busy !== null || !selectedApp || !appLink?.linked}
                  >
                    Unlink app
                  </button>
                </form>
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
      <ConfirmModal
        open={confirmUnlink}
        title="Remove app object store link?"
        description={`This will delete the managed AWS and S3 object store variables from ${selectedApp || 'the selected app'}.`}
        confirmLabel="Unlink app"
        busy={busy === `unlink-${selectedApp}`}
        onClose={() => {
          if (busy !== `unlink-${selectedApp}`) setConfirmUnlink(false);
        }}
        onConfirm={() => {
          void handleUnlink();
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

function fieldErrorId(key: keyof ObjectStoreFieldErrors): string {
  return `objectstore-${key.replace(/_/g, '-')}-error`;
}

function isRequiredDraftField(key: keyof ObjectStoreDraft): key is keyof ObjectStoreFieldErrors {
  return REQUIRED_DRAFT_FIELDS.some((field) => field.key === key);
}

function validateDraft(draft: ObjectStoreDraft): ObjectStoreFieldErrors {
  const errors: ObjectStoreFieldErrors = {};

  if (!draft.provider.trim()) {
    errors.provider = 'Enter the storage provider, such as r2 or s3.';
  }
  if (!draft.bucket.trim()) {
    errors.bucket = 'Enter the bucket name.';
  }
  if (!draft.region.trim()) {
    errors.region = 'Enter the region, or auto when the provider has no regions.';
  }
  if (!draft.endpoint.trim()) {
    errors.endpoint = 'Enter the S3 endpoint URL.';
  }
  if (!draft.access_key_id.trim()) {
    errors.access_key_id = 'Enter the access key ID.';
  }
  if (!draft.secret_access_key.trim()) {
    errors.secret_access_key = 'Enter the secret access key.';
  }

  return errors;
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

function chooseSelectedApp(current: string, apps: App[]): string {
  if (current && apps.some((app) => app.name === current)) {
    return current;
  }

  return apps[0]?.name ?? '';
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric-card">
      <p>{label}</p>
      <strong>{value}</strong>
    </div>
  );
}
