import { useEffect, useState, type FormEvent } from 'react';
import {
  addDomain,
  deleteConfigVar,
  fetchApp,
  fetchConfig,
  fetchDeployments,
  fetchDomains,
  fetchScale,
  getToken,
  removeDomain,
  setConfigVar,
  setScale,
  type App,
  type ConfigVar,
  type Deployment,
  type Domain,
  type ScaleMap,
} from '../lib/api';
import ConnectScreen from './ConnectScreen';
import LogStream from './LogStream';
import StatusBadge from './StatusBadge';

interface AppDataState {
  app: App;
  deployments: Deployment[];
  domains: Domain[];
  config: ConfigVar[];
  scales: ScaleMap;
}

export default function AppDetailPage() {
  const [hasToken, setHasToken] = useState(() => Boolean(getToken()));

  if (!hasToken) {
    return <ConnectScreen onConnected={() => setHasToken(true)} />;
  }

  return <AppDetailInner />;
}

function AppDetailInner() {
  const [appName, setAppName] = useState('');
  const [state, setState] = useState<AppDataState | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [domainDraft, setDomainDraft] = useState('');
  const [configKey, setConfigKey] = useState('');
  const [configValue, setConfigValue] = useState('');
  const [scaleProcess, setScaleProcess] = useState('web');
  const [scaleCount, setScaleCount] = useState('1');
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    const query = new URLSearchParams(window.location.search);
    setAppName(query.get('name') ?? '');
  }, []);

  useEffect(() => {
    if (!appName) {
      setLoading(false);
      return;
    }

    void load();
  }, [appName]);

  async function load() {
    if (!appName) return;
    try {
      setLoading(true);
      setError(null);
      const [app, deployments, domains, config, scales] = await Promise.all([
        fetchApp(appName),
        fetchDeployments(appName),
        fetchDomains(appName),
        fetchConfig(appName),
        fetchScale(appName),
      ]);
      setState({ app, deployments, domains, config, scales });
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to load app details.');
    } finally {
      setLoading(false);
    }
  }

  async function handleAddDomain(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextDomain = domainDraft.trim();
    if (!nextDomain) return;
    try {
      setBusy('domain-add');
      await addDomain(appName, nextDomain);
      setDomainDraft('');
      await load();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to add domain.');
    } finally {
      setBusy(null);
    }
  }

  async function handleRemoveDomain(domain: string) {
    try {
      setBusy(`domain-remove-${domain}`);
      await removeDomain(appName, domain);
      await load();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to remove domain.');
    } finally {
      setBusy(null);
    }
  }

  async function handleSetConfig(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const key = configKey.trim();
    if (!key) return;

    try {
      setBusy('config-set');
      await setConfigVar(appName, key, configValue);
      setConfigKey('');
      setConfigValue('');
      await load();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to set config.');
    } finally {
      setBusy(null);
    }
  }

  async function handleRemoveConfig(key: string) {
    try {
      setBusy(`config-remove-${key}`);
      await deleteConfigVar(appName, key);
      await load();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to remove config.');
    } finally {
      setBusy(null);
    }
  }

  async function handleSetScale(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!state) return;
    const parsedCount = Number(scaleCount);
    if (!Number.isFinite(parsedCount) || parsedCount < 0) {
      setError('Replica count must be a non-negative number.');
      return;
    }

    try {
      setBusy('scale-set');
      await setScale(appName, {
        ...state.scales,
        [scaleProcess.trim() || 'web']: parsedCount,
      });
      await load();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to update scale.');
    } finally {
      setBusy(null);
    }
  }

  if (!appName) {
    return (
      <div className="panel empty-state">
        <h1 className="section-title">Select an app</h1>
        <p className="page-copy">
          Open an app from the dashboard home screen to inspect domains, config, scaling, deployment
          history, and live logs.
        </p>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <span className="loading-spinner" />
        <span>Loading app workspace…</span>
      </div>
    );
  }

  if (error || !state) {
    return (
      <div className="panel error-state">
        Failed to load {appName}: {error ?? 'unknown error'}
      </div>
    );
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <p className="eyebrow">App detail</p>
          <div className="cluster justify-between align-start">
            <div className="stack-sm">
              <h1 className="page-title">{state.app.name}</h1>
              <p className="page-copy">
                Runtime controls, deployment history, config surface, and live output for the
                selected app.
              </p>
            </div>
            <StatusBadge status={state.app.status} />
          </div>
        </div>
        <div className="metrics-grid">
          <Metric label="Domains" value={String(state.domains.length)} />
          <Metric label="Deployments" value={String(state.deployments.length)} />
          <Metric label="Config vars" value={String(state.config.length)} />
          <Metric label="TLS" value={state.app.tls_enabled ? 'On' : 'Off'} />
        </div>
      </section>

      <section className="panel-grid">
        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Routing</p>
            <h2 className="section-title">Domains</h2>
          </div>

          <form onSubmit={handleAddDomain} className="stack-md">
            <div className="form-group">
              <label className="form-label" htmlFor="domain-input">
                Add domain
              </label>
              <input
                id="domain-input"
                className="input"
                placeholder="app.example.com"
                value={domainDraft}
                onChange={(event) => setDomainDraft(event.target.value)}
              />
            </div>
            <div className="form-actions">
              <button className="btn btn-primary" type="submit" disabled={busy === 'domain-add'}>
                {busy === 'domain-add' ? 'Adding…' : 'Add domain'}
              </button>
            </div>
          </form>

          {state.domains.length === 0 ? (
            <p className="text-muted">No domains configured yet.</p>
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <th>Domain</th>
                  <th>Created</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {state.domains.map((domain) => (
                  <tr key={domain.id}>
                    <td>{domain.domain}</td>
                    <td className="font-mono">{formatDate(domain.created_at)}</td>
                    <td>
                      <button
                        className="btn btn-danger btn-sm"
                        onClick={() => handleRemoveDomain(domain.domain)}
                        disabled={busy === `domain-remove-${domain.domain}`}
                        type="button"
                      >
                        Remove
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </article>

        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Runtime</p>
            <h2 className="section-title">Scale</h2>
          </div>

          <form onSubmit={handleSetScale} className="stack-md">
            <div className="panel-grid">
              <div className="form-group">
                <label className="form-label" htmlFor="process-input">
                  Process
                </label>
                <input
                  id="process-input"
                  className="input"
                  value={scaleProcess}
                  onChange={(event) => setScaleProcess(event.target.value)}
                />
              </div>
              <div className="form-group">
                <label className="form-label" htmlFor="scale-input">
                  Replicas
                </label>
                <input
                  id="scale-input"
                  className="input"
                  type="number"
                  min="0"
                  value={scaleCount}
                  onChange={(event) => setScaleCount(event.target.value)}
                />
              </div>
            </div>
            <div className="form-actions">
              <button className="btn btn-primary" type="submit" disabled={busy === 'scale-set'}>
                {busy === 'scale-set' ? 'Saving…' : 'Apply scale'}
              </button>
            </div>
          </form>

          {Object.keys(state.scales).length === 0 ? (
            <p className="text-muted">
              No explicit process scale stored yet. Deku defaults web to one replica.
            </p>
          ) : (
            <dl className="data-grid">
              {Object.entries(state.scales).map(([process, count]) => (
                <div key={process}>
                  <dt>{process}</dt>
                  <dd className="font-mono">{count}</dd>
                </div>
              ))}
            </dl>
          )}
        </article>
      </section>

      <section className="panel-grid">
        <article className="panel stack-md">
          <div className="stack-sm">
            <p className="eyebrow">Environment</p>
            <h2 className="section-title">Config vars</h2>
          </div>

          <form onSubmit={handleSetConfig} className="stack-md">
            <div className="panel-grid">
              <div className="form-group">
                <label className="form-label" htmlFor="config-key">
                  Key
                </label>
                <input
                  id="config-key"
                  className="input"
                  value={configKey}
                  onChange={(event) => setConfigKey(event.target.value)}
                  placeholder="NODE_ENV"
                />
              </div>
              <div className="form-group">
                <label className="form-label" htmlFor="config-value">
                  Value
                </label>
                <input
                  id="config-value"
                  className="input"
                  value={configValue}
                  onChange={(event) => setConfigValue(event.target.value)}
                  placeholder="production"
                />
              </div>
            </div>
            <div className="form-actions">
              <button className="btn btn-primary" type="submit" disabled={busy === 'config-set'}>
                {busy === 'config-set' ? 'Saving…' : 'Set var'}
              </button>
            </div>
          </form>

          {state.config.length === 0 ? (
            <p className="text-muted">No config vars have been stored.</p>
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <th>Key</th>
                  <th>Value</th>
                  <th>Scope</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {state.config.map((item) => (
                  <tr key={item.key}>
                    <td className="font-mono">{item.key}</td>
                    <td className="font-mono">{item.value}</td>
                    <td>{item.is_global ? 'Global' : 'App'}</td>
                    <td>
                      <button
                        className="btn btn-danger btn-sm"
                        type="button"
                        disabled={busy === `config-remove-${item.key}`}
                        onClick={() => handleRemoveConfig(item.key)}
                      >
                        Remove
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </article>

        <article className="panel stack-md">
          <div className="cluster justify-between align-center">
            <div className="stack-sm">
              <p className="eyebrow">History</p>
              <h2 className="section-title">Deployments</h2>
            </div>
            <a
              className="btn btn-secondary btn-sm"
              href={`/deployments?app=${encodeURIComponent(state.app.name)}`}
            >
              Open history
            </a>
          </div>

          {state.deployments.length === 0 ? (
            <p className="text-muted">No deployments recorded yet.</p>
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <th>ID</th>
                  <th>Status</th>
                  <th>Builder</th>
                  <th>Created</th>
                </tr>
              </thead>
              <tbody>
                {state.deployments.slice(0, 6).map((deployment) => (
                  <tr key={deployment.id}>
                    <td>
                      <a
                        href={`/deployments?app=${encodeURIComponent(state.app.name)}&id=${deployment.id}`}
                        className="font-mono"
                      >
                        {deployment.id.slice(0, 8)}
                      </a>
                    </td>
                    <td>
                      <StatusBadge status={deployment.status} size="sm" />
                    </td>
                    <td className="font-mono">{deployment.builder}</td>
                    <td className="font-mono">{formatDate(deployment.created_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </article>
      </section>

      <article className="panel stack-md">
        <div className="stack-sm">
          <p className="eyebrow">Streaming output</p>
          <h2 className="section-title">Live events and logs</h2>
        </div>
        <LogStream appName={state.app.name} />
      </article>
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

function formatDate(value: string): string {
  return new Date(value).toLocaleString('en-US', {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}
