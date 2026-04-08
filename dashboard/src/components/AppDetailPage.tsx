import { type SubmitEvent, useCallback, useEffect, useMemo, useState } from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import {
  type App,
  addDomain,
  type ConfigVar,
  type Deployment,
  type Domain,
  deleteConfigVar,
  fetchApp,
  fetchConfig,
  fetchDeployments,
  fetchDomains,
  fetchPorts,
  fetchProcesses,
  fetchScale,
  type PortMapping,
  type ProcessRecord,
  removeDomain,
  type ScaleMap,
  setConfigVar,
  setScale,
} from '../lib/api';
import AppDeployPanel from './AppDeployPanel';
import AppInfrastructurePanel from './AppInfrastructurePanel';
import AppOperationsPanel from './AppOperationsPanel';
import AppRoutingPanel from './AppRoutingPanel';
import ConnectScreen from './ConnectScreen';
import LogStream from './LogStream';
import StatusBadge from './StatusBadge';
import TableScroll from './TableScroll';

interface AppDataState {
  app: App;
  deployments: Deployment[];
  domains: Domain[];
  ports: PortMapping[];
  config: ConfigVar[];
  scales: ScaleMap;
  processes: ProcessRecord[];
}

type AppTabId = 'deploy' | 'proxy' | 'runtime' | 'config' | 'infra' | 'logs' | 'settings';
type FlashTone = 'success' | 'danger';

interface FlashMessage {
  tone: FlashTone;
  text: string;
}

interface AppTabDefinition {
  id: AppTabId;
  label: string;
  summary: string;
}

const DEFAULT_TAB: AppTabId = 'deploy';

export default function AppDetailPage() {
  const tokenAccess = useTokenAccess();

  if (tokenAccess === 'unknown') {
    return null;
  }

  if (tokenAccess === 'locked') {
    return <ConnectScreen />;
  }

  return <AppDetailInner />;
}

function AppDetailInner() {
  const [appName, setAppName] = useState(() => readAppNameFromLocation());
  const [activeTab, setActiveTab] = useState<AppTabId>(() => readTabFromHash(readLocationHash()));
  const [state, setState] = useState<AppDataState | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [flash, setFlash] = useState<FlashMessage | null>(null);
  const [domainDraft, setDomainDraft] = useState('');
  const [configKey, setConfigKey] = useState('');
  const [configValue, setConfigValue] = useState('');
  const [scaleProcess, setScaleProcess] = useState('web');
  const [scaleCount, setScaleCount] = useState('1');
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    const syncFromLocation = () => {
      setAppName(readAppNameFromLocation());
      setActiveTab(readTabFromHash(readLocationHash()));
    };

    syncFromLocation();

    window.addEventListener('hashchange', syncFromLocation);
    window.addEventListener('popstate', syncFromLocation);
    return () => {
      window.removeEventListener('hashchange', syncFromLocation);
      window.removeEventListener('popstate', syncFromLocation);
    };
  }, []);

  const load = useCallback(async () => {
    if (!appName) return;
    try {
      setLoading(true);
      setLoadError(null);
      const [app, deployments, domains, ports, config, scales, processes] = await Promise.all([
        fetchApp(appName),
        fetchDeployments(appName),
        fetchDomains(appName),
        fetchPorts(appName),
        fetchConfig(appName),
        fetchScale(appName),
        fetchProcesses(appName),
      ]);
      setState({ app, deployments, domains, ports, config, scales, processes });
    } catch (nextError) {
      setLoadError(nextError instanceof Error ? nextError.message : 'Unable to load app details.');
    } finally {
      setLoading(false);
    }
  }, [appName]);

  useEffect(() => {
    if (!appName) {
      setLoading(false);
      return;
    }

    void load();
  }, [appName, load]);

  const tabs = useMemo<AppTabDefinition[]>(() => {
    if (!state) {
      return [
        { id: 'deploy', label: 'Deploy', summary: 'Ship changes' },
        { id: 'proxy', label: 'Proxy', summary: 'Domains and TLS' },
        { id: 'runtime', label: 'Runtime', summary: 'Scale and processes' },
        { id: 'config', label: 'Config', summary: 'Environment vars' },
        { id: 'infra', label: 'Infra', summary: 'Networks and storage' },
        { id: 'logs', label: 'Logs', summary: 'Live output' },
        { id: 'settings', label: 'Settings', summary: 'Metadata and delete' },
      ];
    }

    return [
      {
        id: 'deploy',
        label: 'Deploy',
        summary: `${state.deployments.length} deployment${state.deployments.length === 1 ? '' : 's'}`,
      },
      {
        id: 'proxy',
        label: 'Proxy',
        summary: `${state.domains.length} domain${state.domains.length === 1 ? '' : 's'}`,
      },
      {
        id: 'runtime',
        label: 'Runtime',
        summary: `${state.processes.length} process${state.processes.length === 1 ? '' : 'es'}`,
      },
      {
        id: 'config',
        label: 'Config',
        summary: `${state.config.length} var${state.config.length === 1 ? '' : 's'}`,
      },
      {
        id: 'infra',
        label: 'Infra',
        summary: `${state.ports.length} published port${state.ports.length === 1 ? '' : 's'}`,
      },
      {
        id: 'logs',
        label: 'Logs',
        summary: state.app.status === 'deployed' ? 'Streaming live' : 'Recent events',
      },
      {
        id: 'settings',
        label: 'Settings',
        summary: state.app.locked ? 'Locked' : 'Writable',
      },
    ];
  }, [state]);

  async function handleAddDomain(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextDomain = domainDraft.trim();
    if (!nextDomain || !state || state.app.locked) return;

    try {
      setBusy('domain-add');
      setFlash(null);
      await addDomain(appName, nextDomain);
      setDomainDraft('');
      await load();
      setFlash({ tone: 'success', text: `Added domain ${nextDomain}.` });
    } catch (nextError) {
      setFlash({
        tone: 'danger',
        text: nextError instanceof Error ? nextError.message : 'Unable to add domain.',
      });
    } finally {
      setBusy(null);
    }
  }

  async function handleRemoveDomain(domain: string) {
    if (!state || state.app.locked) return;

    try {
      setBusy(`domain-remove-${domain}`);
      setFlash(null);
      await removeDomain(appName, domain);
      await load();
      setFlash({ tone: 'success', text: `Removed domain ${domain}.` });
    } catch (nextError) {
      setFlash({
        tone: 'danger',
        text: nextError instanceof Error ? nextError.message : 'Unable to remove domain.',
      });
    } finally {
      setBusy(null);
    }
  }

  async function handleSetConfig(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const key = configKey.trim();
    if (!key || !state || state.app.locked) return;

    try {
      setBusy('config-set');
      setFlash(null);
      await setConfigVar(appName, key, configValue);
      setConfigKey('');
      setConfigValue('');
      await load();
      setFlash({ tone: 'success', text: `Updated config var ${key}.` });
    } catch (nextError) {
      setFlash({
        tone: 'danger',
        text: nextError instanceof Error ? nextError.message : 'Unable to set config.',
      });
    } finally {
      setBusy(null);
    }
  }

  async function handleRemoveConfig(key: string) {
    if (!state || state.app.locked) return;

    try {
      setBusy(`config-remove-${key}`);
      setFlash(null);
      await deleteConfigVar(appName, key);
      await load();
      setFlash({ tone: 'success', text: `Removed config var ${key}.` });
    } catch (nextError) {
      setFlash({
        tone: 'danger',
        text: nextError instanceof Error ? nextError.message : 'Unable to remove config.',
      });
    } finally {
      setBusy(null);
    }
  }

  async function handleSetScale(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!state || state.app.locked) return;

    const process = scaleProcess.trim() || 'web';
    const parsedCount = Number(scaleCount);
    if (!Number.isFinite(parsedCount) || parsedCount < 0) {
      setFlash({ tone: 'danger', text: 'Replica count must be a non-negative number.' });
      return;
    }

    try {
      setBusy('scale-set');
      setFlash(null);
      await setScale(appName, {
        ...state.scales,
        [process]: parsedCount,
      });
      await load();
      setFlash({ tone: 'success', text: `Set ${process} scale to ${parsedCount}.` });
    } catch (nextError) {
      setFlash({
        tone: 'danger',
        text: nextError instanceof Error ? nextError.message : 'Unable to update scale.',
      });
    } finally {
      setBusy(null);
    }
  }

  function handleTabChange(nextTab: AppTabId) {
    setActiveTab(nextTab);
    const nextHash = `#${nextTab}`;
    const nextUrl = `${window.location.pathname}${window.location.search}${nextHash}`;
    window.history.replaceState({}, '', nextUrl);
  }

  if (!appName) {
    return (
      <div className="panel empty-state">
        <h1 className="section-title">Select an app</h1>
        <p className="page-copy">
          Open an app from the dashboard home screen to inspect domains, config, scaling,
          deployments, and live logs.
        </p>
      </div>
    );
  }

  if (loading && !state) {
    return <AppDetailSkeleton activeTab={activeTab} />;
  }

  if (!state) {
    return (
      <div className="panel error-state">
        Failed to load {appName}: {loadError ?? 'unknown error'}
      </div>
    );
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <div className="panel-heading panel-heading-top">
            <p className="eyebrow">App detail</p>
            <span className="inventory-summary">Created {formatDate(state.app.created_at)}</span>
          </div>
          <div className="panel-heading">
            <div className="stack-sm panel-heading-copy">
              <h1 className="page-title">{state.app.name}</h1>
              <p className="page-copy">
                Deploy, route, scale, inspect, and manage this app without working through a single
                long page.
              </p>
            </div>
            <StatusBadge status={state.app.status} />
          </div>
        </div>
        <div className="metrics-grid">
          <Metric label="Domains" value={String(state.domains.length)} />
          <Metric label="Ports" value={String(state.ports.length)} />
          <Metric label="Deployments" value={String(state.deployments.length)} />
          <Metric label="TLS" value={state.app.tls_enabled ? 'On' : 'Off'} />
        </div>
      </section>

      <section className="panel stack-md">
        <div className="panel-heading panel-heading-top">
          <div className="stack-sm panel-heading-copy">
            <p className="eyebrow">Sections</p>
            <h2 className="section-title">App categories</h2>
          </div>
          <span className="inventory-summary">
            {tabs.find((tab) => tab.id === activeTab)?.label}
          </span>
        </div>

        <div className="app-tab-grid" role="tablist" aria-label={`${state.app.name} sections`}>
          {tabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              id={`tab-${tab.id}`}
              role="tab"
              aria-selected={activeTab === tab.id}
              aria-controls={`panel-${tab.id}`}
              className={`app-tab-button${activeTab === tab.id ? ' is-selected' : ''}`}
              onClick={() => handleTabChange(tab.id)}
            >
              <strong>{tab.label}</strong>
              <span>{tab.summary}</span>
            </button>
          ))}
        </div>

        {state.app.locked ? (
          <p className="callout callout-warning">
            This app is locked. Mutation controls remain visible, but writes are disabled until the
            lock is lifted.
          </p>
        ) : null}
        {loadError ? <p className="callout callout-danger">{loadError}</p> : null}
        {flash ? <p className={`callout callout-${flash.tone}`}>{flash.text}</p> : null}
      </section>

      <div
        id={`panel-${activeTab}`}
        role="tabpanel"
        aria-labelledby={`tab-${activeTab}`}
        className="section-fade"
      >
        {activeTab === 'deploy' ? (
          <div className="stack-lg">
            <AppDeployPanel
              key={state.app.name}
              appName={state.app.name}
              locked={state.app.locked}
              deployments={state.deployments}
              onRefresh={load}
            />
            <DeploymentHistoryPanel appName={state.app.name} deployments={state.deployments} />
          </div>
        ) : null}

        {activeTab === 'proxy' ? (
          <div className="stack-lg">
            <DomainPanel
              domains={state.domains}
              locked={state.app.locked}
              busy={busy}
              value={domainDraft}
              onDraftChange={setDomainDraft}
              onSubmit={handleAddDomain}
              onRemove={handleRemoveDomain}
            />
            <AppRoutingPanel
              key={`${state.app.name}:${state.domains.length}:${state.app.tls_enabled}`}
              appName={state.app.name}
              locked={state.app.locked}
              onAppRefresh={load}
            />
          </div>
        ) : null}

        {activeTab === 'runtime' ? (
          <RuntimePanel
            appName={state.app.name}
            locked={state.app.locked}
            scales={state.scales}
            processes={state.processes}
            scaleProcess={scaleProcess}
            scaleCount={scaleCount}
            busy={busy}
            onScaleProcessChange={setScaleProcess}
            onScaleCountChange={setScaleCount}
            onSubmit={handleSetScale}
          />
        ) : null}

        {activeTab === 'config' ? (
          <ConfigPanel
            config={state.config}
            locked={state.app.locked}
            configKey={configKey}
            configValue={configValue}
            busy={busy}
            onConfigKeyChange={setConfigKey}
            onConfigValueChange={setConfigValue}
            onSubmit={handleSetConfig}
            onRemove={handleRemoveConfig}
          />
        ) : null}

        {activeTab === 'infra' ? (
          <AppInfrastructurePanel
            key={state.app.name}
            appName={state.app.name}
            locked={state.app.locked}
          />
        ) : null}

        {activeTab === 'logs' ? (
          <article className="panel stack-md">
            <div className="stack-sm">
              <p className="eyebrow">Streaming output</p>
              <h2 className="section-title">Live events and logs</h2>
            </div>
            <LogStream appName={state.app.name} />
          </article>
        ) : null}

        {activeTab === 'settings' ? (
          <AppOperationsPanel
            appId={state.app.id}
            appName={state.app.name}
            createdAt={state.app.created_at}
            locked={state.app.locked}
            status={state.app.status}
          />
        ) : null}
      </div>
    </div>
  );
}

interface DomainPanelProps {
  domains: Domain[];
  locked: boolean;
  busy: string | null;
  value: string;
  onDraftChange: (value: string) => void;
  onSubmit: (event: SubmitEvent<HTMLFormElement>) => Promise<void>;
  onRemove: (domain: string) => Promise<void>;
}

function DomainPanel({
  domains,
  locked,
  busy,
  value,
  onDraftChange,
  onSubmit,
  onRemove,
}: DomainPanelProps) {
  return (
    <article className="panel stack-md">
      <div className="panel-heading">
        <div className="stack-sm panel-heading-copy">
          <p className="eyebrow">Routing</p>
          <h2 className="section-title">Domains</h2>
          <p className="page-copy">
            Attach public hostnames before enabling TLS or publishing upstreams through Angie.
          </p>
        </div>
        <span className="inventory-summary">{domains.length} configured</span>
      </div>

      <form onSubmit={onSubmit} className="stack-md">
        <div className="form-group">
          <label className="form-label" htmlFor="domain-input">
            Add domain
          </label>
          <input
            id="domain-input"
            className="input"
            placeholder="app.example.com"
            value={value}
            onChange={(event) => onDraftChange(event.target.value)}
            disabled={locked || busy !== null}
          />
        </div>
        <div className="form-actions">
          <button
            className="btn btn-primary"
            type="submit"
            disabled={locked || busy !== null || value.trim().length === 0}
          >
            {busy === 'domain-add' ? 'Adding…' : 'Add domain'}
          </button>
        </div>
      </form>

      {domains.length === 0 ? (
        <p className="text-muted">No domains configured yet.</p>
      ) : (
        <TableScroll>
          <table className="table">
            <thead>
              <tr>
                <th>Domain</th>
                <th>Created</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {domains.map((domain) => (
                <tr key={domain.id}>
                  <td>{domain.domain}</td>
                  <td className="font-mono">{formatDate(domain.created_at)}</td>
                  <td>
                    <button
                      className="btn btn-danger btn-sm"
                      onClick={() => {
                        void onRemove(domain.domain);
                      }}
                      disabled={locked || busy === `domain-remove-${domain.domain}`}
                      type="button"
                    >
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </TableScroll>
      )}
    </article>
  );
}

interface RuntimePanelProps {
  appName: string;
  locked: boolean;
  scales: ScaleMap;
  processes: ProcessRecord[];
  scaleProcess: string;
  scaleCount: string;
  busy: string | null;
  onScaleProcessChange: (value: string) => void;
  onScaleCountChange: (value: string) => void;
  onSubmit: (event: SubmitEvent<HTMLFormElement>) => Promise<void>;
}

function RuntimePanel({
  appName,
  locked,
  scales,
  processes,
  scaleProcess,
  scaleCount,
  busy,
  onScaleProcessChange,
  onScaleCountChange,
  onSubmit,
}: RuntimePanelProps) {
  return (
    <section className="panel-grid">
      <article className="panel stack-md">
        <div className="stack-sm">
          <p className="eyebrow">Runtime</p>
          <h2 className="section-title">Scale</h2>
          <p className="page-copy">
            Set desired replica counts per process type. Actual container state appears beside it.
          </p>
        </div>

        <form onSubmit={onSubmit} className="stack-md">
          <div className="panel-grid">
            <div className="form-group">
              <label className="form-label" htmlFor="process-input">
                Process
              </label>
              <input
                id="process-input"
                className="input"
                value={scaleProcess}
                onChange={(event) => onScaleProcessChange(event.target.value)}
                disabled={locked || busy !== null}
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
                onChange={(event) => onScaleCountChange(event.target.value)}
                disabled={locked || busy !== null}
              />
            </div>
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
              {busy === 'scale-set' ? 'Saving…' : 'Apply scale'}
            </button>
          </div>
        </form>

        {Object.keys(scales).length === 0 ? (
          <p className="text-muted">
            No explicit process scale stored yet. Deku defaults web to one replica.
          </p>
        ) : (
          <dl className="data-grid">
            {Object.entries(scales).map(([process, count]) => (
              <div key={process}>
                <dt>{process}</dt>
                <dd className="font-mono">{count}</dd>
              </div>
            ))}
          </dl>
        )}
      </article>

      <article className="panel stack-md">
        <div className="panel-heading panel-heading-top">
          <div className="stack-sm panel-heading-copy">
            <p className="eyebrow">Live runtime</p>
            <h2 className="section-title">Process inventory</h2>
          </div>
          <span className="inventory-summary">
            {processes.length} container{processes.length === 1 ? '' : 's'}
          </span>
        </div>

        {processes.length === 0 ? (
          <p className="text-muted">
            No containers are currently recorded for this app. Deploy or scale the app to inspect
            runtime process state here.
          </p>
        ) : (
          <TableScroll>
            <table className="table">
              <thead>
                <tr>
                  <th>Process</th>
                  <th>Status</th>
                  <th>Port</th>
                  <th>Container</th>
                  <th>Deployment</th>
                  <th>Started</th>
                </tr>
              </thead>
              <tbody>
                {processes.map((process) => (
                  <tr key={process.container_id}>
                    <td className="font-mono">{process.process_type}</td>
                    <td>
                      <StatusBadge status={processStatus(process.status)} size="sm" />
                    </td>
                    <td className="font-mono">
                      {process.host_port > 0 ? String(process.host_port) : 'internal'}
                    </td>
                    <td className="font-mono">{truncateId(process.container_id)}</td>
                    <td>
                      <a
                        href={`/deployments?app=${encodeURIComponent(appName)}&id=${process.deployment_id}`}
                        className="font-mono"
                      >
                        {truncateId(process.deployment_id)}
                      </a>
                    </td>
                    <td className="font-mono">{formatDate(process.created_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableScroll>
        )}
      </article>
    </section>
  );
}

interface ConfigPanelProps {
  config: ConfigVar[];
  locked: boolean;
  configKey: string;
  configValue: string;
  busy: string | null;
  onConfigKeyChange: (value: string) => void;
  onConfigValueChange: (value: string) => void;
  onSubmit: (event: SubmitEvent<HTMLFormElement>) => Promise<void>;
  onRemove: (key: string) => Promise<void>;
}

function ConfigPanel({
  config,
  locked,
  configKey,
  configValue,
  busy,
  onConfigKeyChange,
  onConfigValueChange,
  onSubmit,
  onRemove,
}: ConfigPanelProps) {
  return (
    <article className="panel stack-md">
      <div className="panel-heading">
        <div className="stack-sm panel-heading-copy">
          <p className="eyebrow">Environment</p>
          <h2 className="section-title">Config vars</h2>
          <p className="page-copy">
            Store app-scoped values here. Global keys are shown alongside them for context.
          </p>
        </div>
        <span className="inventory-summary">{config.length} visible</span>
      </div>

      <form onSubmit={onSubmit} className="stack-md">
        <div className="panel-grid">
          <div className="form-group">
            <label className="form-label" htmlFor="config-key">
              Key
            </label>
            <input
              id="config-key"
              className="input"
              value={configKey}
              onChange={(event) => onConfigKeyChange(event.target.value)}
              placeholder="NODE_ENV"
              disabled={locked || busy !== null}
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
              onChange={(event) => onConfigValueChange(event.target.value)}
              placeholder="production"
              disabled={locked || busy !== null}
            />
          </div>
        </div>
        <div className="form-actions">
          <button
            className="btn btn-primary"
            type="submit"
            disabled={locked || busy !== null || configKey.trim().length === 0}
          >
            {busy === 'config-set' ? 'Saving…' : 'Set var'}
          </button>
        </div>
      </form>

      {config.length === 0 ? (
        <p className="text-muted">No config vars have been stored.</p>
      ) : (
        <TableScroll>
          <table className="table">
            <thead>
              <tr>
                <th>Key</th>
                <th>Value</th>
                <th>Scope</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {config.map((item) => (
                <tr key={item.key}>
                  <td className="font-mono">{item.key}</td>
                  <td className="font-mono">{item.value}</td>
                  <td>{item.is_global ? 'Global' : 'App'}</td>
                  <td>
                    <button
                      className="btn btn-danger btn-sm"
                      type="button"
                      disabled={locked || busy === `config-remove-${item.key}`}
                      onClick={() => {
                        void onRemove(item.key);
                      }}
                    >
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </TableScroll>
      )}
    </article>
  );
}

function DeploymentHistoryPanel({
  appName,
  deployments,
}: {
  appName: string;
  deployments: Deployment[];
}) {
  return (
    <article className="panel stack-md">
      <div className="panel-heading panel-heading-top">
        <div className="stack-sm panel-heading-copy">
          <p className="eyebrow">History</p>
          <h2 className="section-title">Deployments</h2>
        </div>
        <a
          className="btn btn-secondary btn-sm"
          href={`/deployments?app=${encodeURIComponent(appName)}`}
        >
          Open history
        </a>
      </div>

      {deployments.length === 0 ? (
        <p className="text-muted">No deployments recorded yet.</p>
      ) : (
        <TableScroll>
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
              {deployments.slice(0, 6).map((deployment) => (
                <tr key={deployment.id}>
                  <td>
                    <a
                      href={`/deployments?app=${encodeURIComponent(appName)}&id=${deployment.id}`}
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
        </TableScroll>
      )}
    </article>
  );
}

function AppDetailSkeleton({ activeTab }: { activeTab: AppTabId }) {
  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <div className="panel-heading panel-heading-top">
            <SkeletonBlock className="h-3 w-20" />
            <SkeletonBlock className="h-5 w-32" />
          </div>
          <div className="panel-heading">
            <div className="stack-sm panel-heading-copy">
              <SkeletonBlock className="h-10 w-56 max-w-full" />
              <SkeletonBlock className="h-4 w-full max-w-lg" />
              <SkeletonBlock className="h-4 w-5/6 max-w-md" />
            </div>
            <SkeletonBlock className="h-8 w-24 rounded-full" />
          </div>
        </div>
        <div className="metrics-grid">
          {skeletonItems('metric', 4).map((item) => (
            <div key={item} className="metric-card stack-sm">
              <SkeletonBlock className="h-3 w-16" />
              <SkeletonBlock className="h-8 w-14" />
            </div>
          ))}
        </div>
      </section>

      <section className="panel stack-md">
        <div className="panel-heading panel-heading-top">
          <div className="stack-sm panel-heading-copy">
            <SkeletonBlock className="h-3 w-16" />
            <SkeletonBlock className="h-7 w-44" />
          </div>
          <SkeletonBlock className="h-5 w-20" />
        </div>

        <div className="app-tab-grid">
          {tabOrder.map((tabId) => (
            <div
              key={`tab-skeleton-${tabId}`}
              className={`app-tab-button${tabId === activeTab ? ' is-selected' : ''}`}
            >
              <SkeletonBlock className="h-4 w-16" />
              <SkeletonBlock className="h-3 w-full" />
            </div>
          ))}
        </div>
      </section>

      <div className="section-fade">{renderAppDetailTabSkeleton(activeTab)}</div>
    </div>
  );
}

function renderAppDetailTabSkeleton(activeTab: AppTabId) {
  switch (activeTab) {
    case 'proxy':
      return (
        <div className="stack-lg">
          <SkeletonPanelHeaderCard summaryWidth="w-24" titleWidth="w-32" bodyLines={2} />
          <section className="panel-grid app-routing-grid">
            <SkeletonPanelHeaderCard summaryWidth="w-24" titleWidth="w-44" bodyLines={2} />
            <SkeletonPanelHeaderCard
              summaryWidth="w-24 rounded-full"
              titleWidth="w-40"
              bodyLines={2}
            />
          </section>
        </div>
      );
    case 'runtime':
      return (
        <section className="panel-grid">
          <SkeletonPanelHeaderCard titleWidth="w-24" bodyLines={2} />
          <SkeletonPanelHeaderCard summaryWidth="w-28" titleWidth="w-40" bodyLines={0} />
        </section>
      );
    case 'config':
      return <SkeletonPanelHeaderCard summaryWidth="w-20" titleWidth="w-32" bodyLines={2} />;
    case 'infra':
      return (
        <section className="stack-lg">
          <div className="panel-grid">
            <SkeletonPanelHeaderCard summaryWidth="w-24" titleWidth="w-40" bodyLines={2} />
            <SkeletonPanelHeaderCard titleWidth="w-44" bodyLines={2} />
          </div>
          <SkeletonPanelHeaderCard summaryWidth="w-24" titleWidth="w-48" bodyLines={1} />
        </section>
      );
    case 'logs':
      return (
        <article className="panel stack-md">
          <div className="stack-sm">
            <SkeletonBlock className="h-3 w-24" />
            <SkeletonBlock className="h-7 w-48" />
          </div>
          <div className="panel log-panel">
            <div className="log-toolbar">
              <div className="log-status">
                <SkeletonBlock className="size-3 rounded-full" />
                <div className="log-status-copy">
                  <SkeletonBlock className="h-3 w-16" />
                  <SkeletonBlock className="h-3 w-20" />
                </div>
              </div>
              <div className="cluster">
                <SkeletonBlock className="h-9 w-28" />
                <SkeletonBlock className="h-9 w-20" />
              </div>
            </div>
            <div className="log-terminal skeleton-log-terminal">
              {skeletonItems('log-line', 6).map((item) => (
                <div key={item} className="log-line">
                  <SkeletonBlock className="h-3 w-12" />
                  <SkeletonBlock className="h-3 w-16" />
                  <SkeletonBlock className="h-3 w-24" />
                  <SkeletonBlock className="h-3 w-full" />
                </div>
              ))}
            </div>
          </div>
        </article>
      );
    case 'settings':
      return (
        <section className="panel-grid">
          <SkeletonPanelHeaderCard titleWidth="w-52" bodyLines={2} />
          <SkeletonPanelHeaderCard titleWidth="w-28" bodyLines={2} />
        </section>
      );
    default:
      return (
        <div className="stack-lg">
          <SkeletonPanelHeaderCard summaryWidth="w-28" titleWidth="w-40" bodyLines={2} />
          <SkeletonPanelHeaderCard titleWidth="w-36" bodyLines={0} actionWidth="w-28" />
        </div>
      );
  }
}

function SkeletonPanelHeaderCard({
  summaryWidth,
  titleWidth,
  bodyLines,
  actionWidth,
}: {
  summaryWidth?: string;
  titleWidth: string;
  bodyLines: number;
  actionWidth?: string;
}) {
  return (
    <article className="panel stack-md">
      <div className="panel-heading">
        <div className="stack-sm panel-heading-copy">
          <SkeletonBlock className="h-3 w-20" />
          <SkeletonBlock className={`h-7 ${titleWidth}`} />
          {skeletonItems('body-line', bodyLines).map((item, index) => (
            <SkeletonBlock
              key={item}
              className={`h-4 ${index === 0 ? 'w-full max-w-md' : 'w-4/5 max-w-sm'}`}
            />
          ))}
        </div>
        {summaryWidth ? <SkeletonBlock className={`h-5 ${summaryWidth}`} /> : null}
      </div>

      <div className="stack-md">
        {skeletonItems('field', 2).map((item) => (
          <div key={item} className="form-group">
            <SkeletonBlock className="h-3 w-24" />
            <SkeletonBlock className="h-11 w-full" />
          </div>
        ))}

        <div className="form-actions">
          <SkeletonBlock className={`h-10 ${actionWidth ?? 'w-32'}`} />
        </div>

        <div className="stack-sm">
          {skeletonItems('row', 3).map((item) => (
            <SkeletonBlock key={item} className="h-12 w-full" />
          ))}
        </div>
      </div>
    </article>
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

function readTabFromHash(hash: string): AppTabId {
  const normalized = hash.replace(/^#/, '');
  return isAppTabId(normalized) ? normalized : DEFAULT_TAB;
}

function isAppTabId(value: string): value is AppTabId {
  return ['deploy', 'proxy', 'runtime', 'config', 'infra', 'logs', 'settings'].includes(value);
}

function readAppNameFromLocation(): string {
  if (typeof window === 'undefined') return '';
  return new URLSearchParams(window.location.search).get('name') ?? '';
}

function readLocationHash(): string {
  if (typeof window === 'undefined') return '';
  return window.location.hash;
}

function SkeletonBlock({ className }: { className: string }) {
  return <div className={`app-skeleton-block animate-pulse rounded-md ${className}`} />;
}

const tabOrder: AppTabId[] = ['deploy', 'proxy', 'runtime', 'config', 'infra', 'logs', 'settings'];

function skeletonItems(prefix: string, count: number): string[] {
  return Array.from({ length: count }, (_, index) => `${prefix}-${index}`);
}

function formatDate(value: string): string {
  return new Date(value).toLocaleString('en-US', {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function truncateId(value: string): string {
  return value.slice(0, 12);
}

function processStatus(
  value: string
): 'created' | 'deployed' | 'stopped' | 'error' | 'building' | 'pending' {
  const normalized = value.toLowerCase();
  if (normalized.includes('run') || normalized.includes('up') || normalized.includes('live')) {
    return 'deployed';
  }
  if (
    normalized.includes('build') ||
    normalized.includes('pull') ||
    normalized.includes('create')
  ) {
    return 'building';
  }
  if (
    normalized.includes('fail') ||
    normalized.includes('error') ||
    normalized.includes('dead') ||
    normalized.includes('exit')
  ) {
    return 'error';
  }
  if (normalized.includes('stop')) {
    return 'stopped';
  }
  return 'pending';
}
