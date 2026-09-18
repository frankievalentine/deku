import {
  type Dispatch,
  type SetStateAction,
  type SubmitEvent,
  useCallback,
  useEffect,
  useState,
} from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import type { ObjectStoreConfig, ObjectStoreState } from '../lib/api';
import {
  fetchObjectStoreConfig,
  setObjectStoreConfig,
  testObjectStoreConfig,
  unsetObjectStoreConfig,
} from '../lib/api';
import { findProviderPreset, OBJECT_STORE_PROVIDERS } from '../lib/object-store-providers';
import ConfirmModal from './ConfirmModal';
import ConnectScreen from './ConnectScreen';
import SelectField from './SelectField';
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

export default function StoragePage() {
  const tokenAccess = useTokenAccess();

  if (tokenAccess === 'unknown') {
    return null;
  }

  if (tokenAccess === 'locked') {
    return <ConnectScreen />;
  }

  return <StorageInner />;
}

function StorageInner() {
  const [state, setState] = useState<ObjectStoreState | null>(null);
  const [draft, setDraft] = useState<ObjectStoreDraft>(EMPTY_DRAFT);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<ObjectStoreFieldErrors>({});
  const [notice, setNotice] = useState<string | null>(null);
  const [confirmUnset, setConfirmUnset] = useState(false);

  // A saved config may name a provider the preset list does not know. In that
  // case the stored string is kept as its own option so it is never rewritten
  // just by opening the page.
  const customProvider =
    draft.provider && !findProviderPreset(draft.provider) ? draft.provider : null;
  const providerValue = findProviderPreset(draft.provider) ? draft.provider : 'custom';
  const providerPreset = findProviderPreset(providerValue) ?? OBJECT_STORE_PROVIDERS[0];

  function handleProviderChange(next: string) {
    const preset = findProviderPreset(next);
    setDraft((current) => ({
      ...current,
      provider: next,
      region: preset ? preset.defaultRegion : current.region,
      path_style: preset ? preset.pathStyle : current.path_style,
    }));
    setFieldErrors((current) => {
      const { provider: _provider, ...rest } = current;
      return rest;
    });
  }

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
      const nextState = await fetchObjectStoreConfig();
      setState(nextState);
      setDraft(buildDraft(nextState.object_store));
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

  async function handleSave(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();

    // Validate first: a value can be present and still wrong for the chosen
    // provider, so a non-null payload is not proof of a usable config.
    const nextErrors = validateDraft(draft);
    if (Object.keys(nextErrors).length > 0) {
      setFieldErrors(nextErrors);
      const firstMissing = REQUIRED_DRAFT_FIELDS.find((field) => nextErrors[field.key]);
      if (firstMissing) {
        document.getElementById(firstMissing.inputId)?.focus();
      }
      return;
    }

    const payload = buildPayload(draft);
    if (!payload) return;

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

  if (loading) {
    return (
      <div className="panel loading-state">
        <Spinner />
        <span>Loading storage configuration…</span>
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
            <h1 className="page-title">Storage</h1>
            <p className="page-copy">
              Where backups are stored. Works with any S3-compatible service.
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

        <article className="panel stack-md">
          <div className="panel-heading">
            <div className="stack-sm panel-heading-copy">
              <p className="eyebrow">Object storage</p>
              <h2 className="section-title">Connection details</h2>
              <p className="page-copy">
                Only a redacted copy of the secret access key is kept, so enter the secret again on
                every save.
              </p>
            </div>
            <span className="inventory-summary">
              {state.configured ? 'Configured' : 'Not configured'}
            </span>
          </div>

          <form onSubmit={handleSave} className="stack-md" noValidate>
            <div className="panel-grid">
              <div className="form-group">
                <label className="form-label" htmlFor="objectstore-provider">
                  Provider
                </label>
                <SelectField
                  id="objectstore-provider"
                  value={providerValue}
                  onChange={handleProviderChange}
                  disabled={busy !== null}
                  options={[
                    ...OBJECT_STORE_PROVIDERS.map((provider) => ({
                      value: provider.value,
                      label: provider.label,
                    })),
                    ...(customProvider
                      ? [{ value: customProvider, label: `${customProvider} (saved)` }]
                      : []),
                  ]}
                />
                <p className="text-muted">{providerPreset.hint}</p>
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
                <label className="form-label" htmlFor="objectstore-endpoint">
                  Endpoint
                </label>
                <input
                  id="objectstore-endpoint"
                  className="input"
                  value={draft.endpoint}
                  onChange={(event) => updateField('endpoint', event.target.value)}
                  placeholder={providerPreset.endpointPlaceholder}
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
                <label className="form-label" htmlFor="objectstore-region">
                  Region
                </label>
                <input
                  id="objectstore-region"
                  className="input"
                  value={draft.region}
                  onChange={(event) => updateField('region', event.target.value)}
                  placeholder={providerPreset.regionPlaceholder}
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
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => {
                  void handleTest();
                }}
                disabled={!state.configured || busy !== null}
              >
                {busy === 'test' ? <span className="loading-spinner" /> : null}
                <span>Test saved configuration</span>
              </button>
            </div>
          </form>

          <div className="stack-sm">
            <p className="text-muted">
              Removing the configuration stops managed backup restores until a new one is saved.
            </p>
            <div className="form-actions">
              <button
                type="button"
                className="btn btn-danger"
                onClick={() => setConfirmUnset(true)}
                disabled={!state.configured || busy !== null}
              >
                Remove configuration
              </button>
            </div>
          </div>
        </article>
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

function fieldErrorId(key: keyof ObjectStoreFieldErrors): string {
  return `objectstore-${key.replace(/_/g, '-')}-error`;
}

function isRequiredDraftField(key: keyof ObjectStoreDraft): key is keyof ObjectStoreFieldErrors {
  return REQUIRED_DRAFT_FIELDS.some((field) => field.key === key);
}

function validateDraft(draft: ObjectStoreDraft): ObjectStoreFieldErrors {
  const errors: ObjectStoreFieldErrors = {};
  const preset = findProviderPreset(draft.provider);

  if (!draft.bucket.trim()) {
    errors.bucket = 'Enter the bucket name.';
  }
  if (!draft.region.trim()) {
    errors.region = 'Enter the region, or auto when the provider has no regions.';
  }
  if (!draft.endpoint.trim()) {
    errors.endpoint = 'Enter the S3 endpoint URL.';
  } else if (preset?.endpointPattern && !preset.endpointPattern.test(draft.endpoint.trim())) {
    errors.endpoint =
      preset.endpointPatternMessage ?? 'That endpoint does not match this provider.';
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

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric-card">
      <p>{label}</p>
      <strong>{value}</strong>
    </div>
  );
}
