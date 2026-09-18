// biome-ignore-all lint/a11y/noNoninteractiveElementToInteractiveRole: Basecoat's documented tabs contract is <nav role="tablist">, with each tab button carrying role="tab".
import { useQueryClient } from '@tanstack/react-query';
import {
  type ReactNode,
  type RefObject,
  type SubmitEvent,
  useCallback,
  useEffect,
  useEffectEvent,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import type {
  App,
  ConfigVar,
  Deployment,
  Domain,
  Environment,
  PortMapping,
  ProcessRecord,
  ScaleMap,
} from '../lib/api';
import {
  getErrorMessage,
  getFirstQueryError,
  invalidateAppDetailQueries,
  useAddDomainMutation,
  useAppConfigQuery,
  useAppDeploymentsQuery,
  useAppDomainsQuery,
  useAppEnvironmentsQuery,
  useAppPortsQuery,
  useAppProcessesQuery,
  useAppScaleQuery,
  useAppSummaryQuery,
  useDeleteConfigVarMutation,
  useRemoveDomainMutation,
  useSetConfigVarMutation,
  useSetScaleMutation,
} from '../lib/query';
import AppConsolePanel from './AppConsolePanel';
import AppDeployPanel from './AppDeployPanel';
import AppDeployTokensPanel from './AppDeployTokensPanel';
import AppEnvironmentsPanel from './AppEnvironmentsPanel';
import AppHealthChecksPanel from './AppHealthChecksPanel';
import AppInfrastructurePanel from './AppInfrastructurePanel';
import AppLimitsPanel from './AppLimitsPanel';
import AppObjectStorePanel from './AppObjectStorePanel';
import AppOperationsPanel from './AppOperationsPanel';
import AppRoutingPanel from './AppRoutingPanel';
import AppTrafficPanel from './AppTrafficPanel';
import ConnectScreen from './ConnectScreen';
import LogStream from './LogStream';
import SelectField from './SelectField';
import StatusBadge from './StatusBadge';
import TableScroll from './TableScroll';

interface AppDataState {
  app: App;
  deployments: Deployment[];
  environments: Environment[];
  domains: Domain[];
  ports: PortMapping[];
  config: ConfigVar[];
  scales: ScaleMap;
  processes: ProcessRecord[];
}

type AppTabId =
  | 'deploy'
  | 'proxy'
  | 'traffic'
  | 'runtime'
  | 'config'
  | 'infra'
  | 'logs'
  | 'settings';
type FlashTone = 'success' | 'danger';

interface FlashMessage {
  tone: FlashTone;
  text: string;
}

interface AppTabDefinition {
  id: AppTabId;
  label: string;
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
  const queryClient = useQueryClient();
  const [appName, setAppName] = useState(() => readAppNameFromLocation());
  const [activeTab, setActiveTab] = useState<AppTabId>(() => readTabFromHash(readLocationHash()));
  const [flash, setFlash] = useState<FlashMessage | null>(null);
  const [domainDraft, setDomainDraft] = useState('');
  const [domainError, setDomainError] = useState<string | null>(null);
  const [configKey, setConfigKey] = useState('');
  const [configValue, setConfigValue] = useState('');
  const [configError, setConfigError] = useState<string | null>(null);
  const [scaleProcess, setScaleProcess] = useState('web');
  const [scaleCount, setScaleCount] = useState('1');
  const [scaleError, setScaleError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  // Empty means app-wide for config and production for deploys, which is what
  // the API does when no environment is named.
  const [environment, setEnvironment] = useState('');
  const [tabsScrollable, setTabsScrollable] = useState(false);
  const tabStripRef = useRef<HTMLDivElement | null>(null);
  const activeTabRef = useRef(activeTab);
  activeTabRef.current = activeTab;
  const domainInputRef = useRef<HTMLInputElement | null>(null);
  const configKeyInputRef = useRef<HTMLInputElement | null>(null);
  const scaleCountInputRef = useRef<HTMLInputElement | null>(null);
  const appQuery = useAppSummaryQuery(appName, { enabled: Boolean(appName) });
  const deploymentsQuery = useAppDeploymentsQuery(appName, {
    enabled: Boolean(appName),
    refetchInterval: 15_000,
  });
  const domainsQuery = useAppDomainsQuery(appName, { enabled: Boolean(appName) });
  const portsQuery = useAppPortsQuery(appName, { enabled: Boolean(appName) });
  const environmentsQuery = useAppEnvironmentsQuery(appName, { enabled: Boolean(appName) });
  const configQuery = useAppConfigQuery(appName, environment || undefined, {
    enabled: Boolean(appName),
  });
  const scaleQuery = useAppScaleQuery(appName, {
    enabled: Boolean(appName),
    refetchInterval: 15_000,
  });
  const processesQuery = useAppProcessesQuery(appName, {
    enabled: Boolean(appName),
    refetchInterval: 15_000,
  });
  const addDomainMutation = useAddDomainMutation(appName);
  const removeDomainMutation = useRemoveDomainMutation(appName);
  const setConfigMutation = useSetConfigVarMutation(appName, environment || undefined);
  const deleteConfigMutation = useDeleteConfigVarMutation(appName, environment || undefined);
  const setScaleMutation = useSetScaleMutation(appName);

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

  // Basecoat's tabs own keyboard navigation and selection on the DOM, and it
  // exposes no change event, so mirror the selected tab back into React state
  // and the hash. Without this, a re-render would reset Basecoat's selection.
  const syncTabFromList = useEffectEvent((nextTab: AppTabId) => {
    handleTabChange(nextTab);
  });

  const state = useMemo<AppDataState | null>(() => {
    if (
      !appQuery.data ||
      !deploymentsQuery.data ||
      !domainsQuery.data ||
      !portsQuery.data ||
      !configQuery.data ||
      !environmentsQuery.data ||
      !scaleQuery.data ||
      !processesQuery.data
    ) {
      return null;
    }

    return {
      app: appQuery.data,
      deployments: deploymentsQuery.data,
      environments: environmentsQuery.data,
      domains: domainsQuery.data,
      ports: portsQuery.data,
      config: configQuery.data,
      scales: scaleQuery.data,
      processes: processesQuery.data,
    };
  }, [
    appQuery.data,
    configQuery.data,
    deploymentsQuery.data,
    domainsQuery.data,
    environmentsQuery.data,
    portsQuery.data,
    processesQuery.data,
    scaleQuery.data,
  ]);

  const loading =
    Boolean(appName) &&
    !state &&
    [
      appQuery,
      deploymentsQuery,
      domainsQuery,
      portsQuery,
      configQuery,
      environmentsQuery,
      scaleQuery,
      processesQuery,
    ].some((query) => query.isPending);

  const loadError = getFirstQueryError(
    [
      appQuery.error,
      deploymentsQuery.error,
      domainsQuery.error,
      portsQuery.error,
      configQuery.error,
      environmentsQuery.error,
      scaleQuery.error,
      processesQuery.error,
    ],
    state ? null : 'Unable to load app details.'
  );

  const refreshApp = useCallback(async () => {
    if (!appName) return;
    await invalidateAppDetailQueries(queryClient, appName);
  }, [appName, queryClient]);

  useEffect(() => {
    if (!state) return;

    const strip = tabStripRef.current?.querySelector('.app-tabs-track');
    if (!strip) return;

    const measure = () => {
      setTabsScrollable(strip.scrollWidth - strip.clientWidth > 4);
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(strip);
    return () => observer.disconnect();
  }, [state]);

  useEffect(() => {
    if (!state) return;

    const strip = tabStripRef.current?.querySelector('.app-tabs-track');
    const tab = document.getElementById(`tab-${activeTab}`);
    if (!strip || !tab) return;

    const stripStart = strip.scrollLeft;
    const stripEnd = stripStart + strip.clientWidth;
    const tabStart = tab.offsetLeft;
    const tabEnd = tabStart + tab.offsetWidth;

    if (tabStart >= stripStart && tabEnd <= stripEnd) return;

    strip.scrollTo({
      left: Math.max(0, tabStart - (strip.clientWidth - tab.offsetWidth) / 2),
      behavior: 'auto',
    });
  }, [activeTab, state]);

  useEffect(() => {
    if (!state) return;

    const list = tabStripRef.current?.querySelector('[role="tablist"]');
    if (!list) return;

    const observer = new MutationObserver(() => {
      const selected = list.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
      const nextTab = selected?.id.replace(/^tab-/, '') as AppTabId | undefined;
      if (!nextTab || !tabOrder.includes(nextTab) || nextTab === activeTabRef.current) return;
      syncTabFromList(nextTab);
    });
    observer.observe(list, { subtree: true, attributeFilter: ['aria-selected'] });
    return () => observer.disconnect();
  }, [state]);

  const tabs = useMemo<AppTabDefinition[]>(() => {
    const labels: AppTabId[] = [
      'deploy',
      'proxy',
      'traffic',
      'runtime',
      'config',
      'infra',
      'logs',
      'settings',
    ];
    const names: Record<AppTabId, string> = {
      deploy: 'Deploy',
      proxy: 'Domains',
      traffic: 'Traffic',
      runtime: 'Processes',
      config: 'Config',
      infra: 'Infrastructure',
      logs: 'Logs',
      settings: 'Settings',
    };
    return labels.map((id) => ({ id, label: names[id] }));
  }, []);

  async function handleAddDomain(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextDomain = domainDraft.trim();
    if (!state || state.app.locked) return;

    if (!nextDomain) {
      setDomainError('Enter a domain to add it to this app.');
      domainInputRef.current?.focus();
      return;
    }

    setDomainError(null);

    try {
      setBusy('domain-add');
      setFlash(null);
      await addDomainMutation.mutateAsync(nextDomain);
      setDomainDraft('');
      setFlash({ tone: 'success', text: `Added domain ${nextDomain}.` });
    } catch (nextError) {
      setFlash({
        tone: 'danger',
        text: getErrorMessage(nextError, 'Unable to add domain.'),
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
      await removeDomainMutation.mutateAsync(domain);
      setFlash({ tone: 'success', text: `Removed domain ${domain}.` });
    } catch (nextError) {
      setFlash({
        tone: 'danger',
        text: getErrorMessage(nextError, 'Unable to remove domain.'),
      });
    } finally {
      setBusy(null);
    }
  }

  async function handleSetConfig(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const key = configKey.trim();
    if (!state || state.app.locked) return;

    if (!key) {
      setConfigError('Enter a config var key.');
      configKeyInputRef.current?.focus();
      return;
    }

    setConfigError(null);

    try {
      setBusy('config-set');
      setFlash(null);
      await setConfigMutation.mutateAsync({ key, value: configValue });
      setConfigKey('');
      setConfigValue('');
      setFlash({ tone: 'success', text: `Updated config var ${key}.` });
    } catch (nextError) {
      setFlash({
        tone: 'danger',
        text: getErrorMessage(nextError, 'Unable to set config.'),
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
      await deleteConfigMutation.mutateAsync(key);
      setFlash({ tone: 'success', text: `Removed config var ${key}.` });
    } catch (nextError) {
      setFlash({
        tone: 'danger',
        text: getErrorMessage(nextError, 'Unable to remove config.'),
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
    if (scaleCount.trim().length === 0 || !Number.isInteger(parsedCount) || parsedCount < 0) {
      setScaleError('Enter a whole number of replicas, 0 or more.');
      scaleCountInputRef.current?.focus();
      return;
    }

    setScaleError(null);

    try {
      setBusy('scale-set');
      setFlash(null);
      await setScaleMutation.mutateAsync({
        ...state.scales,
        [process]: parsedCount,
      });
      setFlash({ tone: 'success', text: `Set ${process} scale to ${parsedCount}.` });
    } catch (nextError) {
      setFlash({
        tone: 'danger',
        text: getErrorMessage(nextError, 'Unable to update scale.'),
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
      <header className="apps-header">
        <div className="apps-header-top">
          <div className="apps-header-copy">
            <h1 className="page-title">{state.app.name}</h1>
            <p className="page-copy">Created {formatDate(state.app.created_at)}</p>
          </div>
          <StatusBadge status={state.app.status} />
        </div>
        <ul className="apps-metrics">
          <AppMetric label="Domains" value={String(state.domains.length)} />
          <AppMetric label="Ports" value={String(state.ports.length)} />
          <AppMetric label="Deployments" value={String(state.deployments.length)} />
          <AppMetric label="TLS" value={state.app.tls_enabled ? 'On' : 'Off'} />
        </ul>
      </header>

      <section className="panel stack-md">
        <div className={`tabs app-tabs${tabsScrollable ? ' is-scrollable' : ''}`} ref={tabStripRef}>
          <nav
            className="app-tabs-track"
            role="tablist"
            aria-orientation="horizontal"
            aria-label={`${state.app.name} sections`}
          >
            {tabs.map((tab) => (
              <button
                key={tab.id}
                type="button"
                id={`tab-${tab.id}`}
                role="tab"
                aria-selected={activeTab === tab.id}
                aria-controls={`panel-${tab.id}`}
                tabIndex={activeTab === tab.id ? 0 : -1}
                className="app-tab"
                onClick={() => handleTabChange(tab.id)}
              >
                {tab.label}
              </button>
            ))}
          </nav>

          {state.app.locked ? (
            <p className="callout callout-warning">
              This app is locked. Changes stay visible but are disabled until it is unlocked.
            </p>
          ) : null}
          {loadError ? <p className="callout callout-danger">{loadError}</p> : null}
          {flash ? <p className={`callout callout-${flash.tone}`}>{flash.text}</p> : null}

          <TabPanel id="deploy" active={activeTab}>
            <div className="stack-lg">
              <AppDeployPanel
                key={state.app.name}
                appName={state.app.name}
                locked={state.app.locked}
                deployments={state.deployments}
                environments={state.environments}
                environment={environment}
                onEnvironmentChange={setEnvironment}
                onRefresh={refreshApp}
              />
              <DeploymentHistoryPanel appName={state.app.name} deployments={state.deployments} />
              <AppHealthChecksPanel appName={state.app.name} locked={state.app.locked} />
              <AppEnvironmentsPanel
                appName={state.app.name}
                locked={state.app.locked}
                environments={state.environments}
                onRefresh={refreshApp}
              />
              <AppDeployTokensPanel appName={state.app.name} locked={state.app.locked} />
            </div>
          </TabPanel>

          <TabPanel id="proxy" active={activeTab}>
            <div className="stack-lg">
              <DomainPanel
                domains={state.domains}
                locked={state.app.locked}
                busy={busy}
                value={domainDraft}
                error={domainError}
                inputRef={domainInputRef}
                onDraftChange={setDomainDraft}
                onSubmit={handleAddDomain}
                onRemove={handleRemoveDomain}
              />
              <AppRoutingPanel
                key={`${state.app.name}:${state.domains.length}:${state.app.tls_enabled}`}
                appName={state.app.name}
                locked={state.app.locked}
                onAppRefresh={refreshApp}
              />
            </div>
          </TabPanel>

          <TabPanel id="traffic" active={activeTab}>
            <AppTrafficPanel
              appName={state.app.name}
              locked={state.app.locked}
              onAppRefresh={refreshApp}
            />
          </TabPanel>

          <TabPanel id="runtime" active={activeTab}>
            <section className="panel-grid">
              <RuntimePanel
                locked={state.app.locked}
                scales={state.scales}
                scaleProcess={scaleProcess}
                scaleCount={scaleCount}
                error={scaleError}
                countInputRef={scaleCountInputRef}
                busy={busy}
                onScaleProcessChange={setScaleProcess}
                onScaleCountChange={setScaleCount}
                onSubmit={handleSetScale}
              />
              <AppLimitsPanel appName={state.app.name} locked={state.app.locked} />
              <ProcessInventory appName={state.app.name} processes={state.processes} />
              <AppConsolePanel
                appName={state.app.name}
                locked={state.app.locked}
                className="panel-span-full"
              />
            </section>
          </TabPanel>

          <TabPanel id="config" active={activeTab}>
            <div className="stack-lg">
              <ConfigPanel
                config={state.config}
                environments={state.environments}
                environment={environment}
                onEnvironmentChange={setEnvironment}
                locked={state.app.locked}
                configKey={configKey}
                configValue={configValue}
                error={configError}
                keyInputRef={configKeyInputRef}
                busy={busy}
                onConfigKeyChange={setConfigKey}
                onConfigValueChange={setConfigValue}
                onSubmit={handleSetConfig}
                onRemove={handleRemoveConfig}
              />
              <AppObjectStorePanel appName={state.app.name} locked={state.app.locked} />
            </div>
          </TabPanel>

          <TabPanel id="infra" active={activeTab}>
            <AppInfrastructurePanel
              key={state.app.name}
              appName={state.app.name}
              locked={state.app.locked}
            />
          </TabPanel>

          <TabPanel id="logs" active={activeTab}>
            <article className="panel stack-md">
              <div className="stack-sm">
                <p className="eyebrow">Streaming output</p>
                <h2 className="section-title">Live events and logs</h2>
              </div>
              <LogStream appName={state.app.name} />
            </article>
          </TabPanel>

          <TabPanel id="settings" active={activeTab}>
            <AppOperationsPanel
              appId={state.app.id}
              appName={state.app.name}
              createdAt={state.app.created_at}
              locked={state.app.locked}
              status={state.app.status}
            />
          </TabPanel>
        </div>
      </section>
    </div>
  );
}

interface TabPanelProps {
  id: AppTabId;
  active: AppTabId;
  children: ReactNode;
}

function TabPanel({ id, active, children }: TabPanelProps) {
  return (
    <div
      id={`panel-${id}`}
      role="tabpanel"
      aria-labelledby={`tab-${id}`}
      tabIndex={-1}
      hidden={active !== id}
    >
      {active === id ? <div className="section-fade">{children}</div> : null}
    </div>
  );
}

interface DomainPanelProps {
  domains: Domain[];
  locked: boolean;
  busy: string | null;
  value: string;
  error: string | null;
  inputRef: RefObject<HTMLInputElement | null>;
  onDraftChange: (value: string) => void;
  onSubmit: (event: SubmitEvent<HTMLFormElement>) => Promise<void>;
  onRemove: (domain: string) => Promise<void>;
}

function DomainPanel({
  domains,
  locked,
  busy,
  value,
  error,
  inputRef,
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
            ref={inputRef}
            className="input"
            placeholder="app.example.com"
            value={value}
            onChange={(event) => onDraftChange(event.target.value)}
            aria-invalid={error ? true : undefined}
            aria-describedby={error ? 'domain-input-error' : undefined}
            disabled={locked || busy !== null}
          />
          {error ? (
            <p id="domain-input-error" className="form-error">
              {error}
            </p>
          ) : null}
        </div>
        <div className="form-actions">
          <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
            {busy === 'domain-add' ? 'Adding…' : 'Add domain'}
          </button>
        </div>
      </form>

      {domains.length === 0 ? (
        <p className="text-muted">No domains configured yet.</p>
      ) : (
        <TableScroll>
          <table className="table">
            <caption className="sr-only">Domains configured for this app</caption>
            <thead>
              <tr>
                <th scope="col">Domain</th>
                <th scope="col">Created</th>
                <th scope="col">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {domains.map((domain) => (
                <tr key={domain.id}>
                  <td>{domain.domain}</td>
                  <td className="font-mono">{formatDate(domain.created_at)}</td>
                  <td>
                    <button
                      className="btn btn-outline btn-danger-outline btn-sm"
                      type="button"
                      aria-label={`Remove domain ${domain.domain}`}
                      onClick={() => {
                        void onRemove(domain.domain);
                      }}
                      disabled={locked || busy === `domain-remove-${domain.domain}`}
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
  locked: boolean;
  scales: ScaleMap;
  scaleProcess: string;
  scaleCount: string;
  error: string | null;
  countInputRef: RefObject<HTMLInputElement | null>;
  busy: string | null;
  onScaleProcessChange: (value: string) => void;
  onScaleCountChange: (value: string) => void;
  onSubmit: (event: SubmitEvent<HTMLFormElement>) => Promise<void>;
}

function RuntimePanel({
  locked,
  scales,
  scaleProcess,
  scaleCount,
  error,
  countInputRef,
  busy,
  onScaleProcessChange,
  onScaleCountChange,
  onSubmit,
}: RuntimePanelProps) {
  return (
    <>
      <article className="panel stack-md">
        <div className="stack-sm">
          <p className="eyebrow">Runtime</p>
          <h2 className="section-title">Scale</h2>
          <p className="page-copy">
            Set desired replica counts per process type. Actual container state appears below.
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
                ref={countInputRef}
                className="input"
                type="number"
                min="0"
                value={scaleCount}
                onChange={(event) => onScaleCountChange(event.target.value)}
                aria-invalid={error ? true : undefined}
                aria-describedby={error ? 'scale-input-error' : undefined}
                disabled={locked || busy !== null}
              />
              {error ? (
                <p id="scale-input-error" className="form-error">
                  {error}
                </p>
              ) : null}
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
    </>
  );
}

interface ProcessInventoryProps {
  appName: string;
  processes: ProcessRecord[];
}

function ProcessInventory({ appName, processes }: ProcessInventoryProps) {
  return (
    <article className="panel stack-md panel-span-full">
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
            <caption className="sr-only">Running containers for this app</caption>
            <thead>
              <tr>
                <th scope="col">Process</th>
                <th scope="col">Status</th>
                <th scope="col">Port</th>
                <th scope="col">Container</th>
                <th scope="col">Deployment</th>
                <th scope="col">Started</th>
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
  );
}

interface ConfigPanelProps {
  config: ConfigVar[];
  environments: Environment[];
  environment: string;
  onEnvironmentChange: (value: string) => void;
  locked: boolean;
  configKey: string;
  configValue: string;
  error: string | null;
  keyInputRef: RefObject<HTMLInputElement | null>;
  busy: string | null;
  onConfigKeyChange: (value: string) => void;
  onConfigValueChange: (value: string) => void;
  onSubmit: (event: SubmitEvent<HTMLFormElement>) => Promise<void>;
  onRemove: (key: string) => Promise<void>;
}

function ConfigPanel({
  config,
  environments,
  environment,
  onEnvironmentChange,
  locked,
  configKey,
  configValue,
  error,
  keyInputRef,
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

      <div className="form-group">
        <label className="form-label" htmlFor="config-environment">
          Viewing
        </label>
        <SelectField
          id="config-environment"
          value={environment}
          onChange={onEnvironmentChange}
          placeholder="App-wide values"
          disabled={busy !== null}
          options={[
            { value: '', label: 'App-wide values' },
            ...environments
              .filter((entry) => !entry.is_production)
              .map((entry) => ({ value: entry.slug, label: entry.name })),
          ]}
        />
        <p className="text-muted">
          {environment
            ? `Values below are what ${environment} deploys with. Saving writes an override that applies only here.`
            : 'Saving here writes values every environment inherits.'}
        </p>
      </div>

      <form onSubmit={onSubmit} className="stack-md">
        <div className="panel-grid">
          <div className="form-group">
            <label className="form-label" htmlFor="config-key">
              Key
            </label>
            <input
              id="config-key"
              ref={keyInputRef}
              className="input"
              value={configKey}
              onChange={(event) => onConfigKeyChange(event.target.value)}
              placeholder="NODE_ENV"
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? 'config-key-error' : undefined}
              autoComplete="off"
              spellCheck={false}
              disabled={locked || busy !== null}
            />
            {error ? (
              <p id="config-key-error" className="form-error">
                {error}
              </p>
            ) : null}
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
          <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
            {busy === 'config-set' ? 'Saving…' : 'Set var'}
          </button>
        </div>
      </form>

      {config.length === 0 ? (
        <p className="text-muted">No config vars have been stored.</p>
      ) : (
        <TableScroll>
          <table className="table">
            <caption className="sr-only">Config vars visible to this app</caption>
            <thead>
              <tr>
                <th scope="col">Key</th>
                <th scope="col">Value</th>
                <th scope="col">Scope</th>
                <th scope="col">At rest</th>
                <th scope="col">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {config.map((item) => (
                <tr key={item.key}>
                  <td className="font-mono">{item.key}</td>
                  <td className="font-mono">
                    {item.error ? (
                      <span className="text-danger">Unreadable: {item.error}</span>
                    ) : (
                      item.value
                    )}
                  </td>
                  <td>
                    {item.source === 'environment' ? 'Override' : item.is_global ? 'Global' : 'App'}
                  </td>
                  <td>{item.encrypted ? 'Encrypted' : 'Plain'}</td>
                  <td>
                    <button
                      className="btn btn-outline btn-danger-outline btn-sm"
                      type="button"
                      aria-label={`Remove config var ${item.key}`}
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
            <caption className="sr-only">Recent deployments for this app</caption>
            <thead>
              <tr>
                <th scope="col">ID</th>
                <th scope="col">Status</th>
                <th scope="col">Builder</th>
                <th scope="col">URL</th>
                <th scope="col">Created</th>
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
                  <td className="font-mono">
                    {deployment.preview_url ? (
                      <a href={deployment.preview_url} target="_blank" rel="noreferrer">
                        {deployment.preview_url.replace(/^https?:\/\//, '')}
                      </a>
                    ) : (
                      <span className="text-muted">-</span>
                    )}
                  </td>
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
      <header className="apps-header">
        <div className="apps-header-top">
          <div className="apps-header-copy">
            <SkeletonBlock className="h-3 w-12" />
            <SkeletonBlock className="h-9 w-56 max-w-full" />
            <SkeletonBlock className="h-4 w-full max-w-lg" />
          </div>
          <SkeletonBlock className="h-8 w-24 rounded-full" />
        </div>
        <ul className="apps-metrics">
          {skeletonItems('metric', 4).map((item) => (
            <li key={item} className="apps-metric">
              <SkeletonBlock className="h-6 w-10" />
              <SkeletonBlock className="h-3 w-16" />
            </li>
          ))}
        </ul>
      </header>

      <section className="panel stack-md">
        <div className="app-tabs">
          <div className="app-tabs-track">
            {tabOrder.map((tabId) => (
              <div
                key={`tab-skeleton-${tabId}`}
                className={`app-tab${tabId === activeTab ? ' is-selected' : ''}`}
              >
                <SkeletonBlock className="h-4 w-16" />
                <SkeletonBlock className="h-3 w-20" />
              </div>
            ))}
          </div>
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
        <section className="stack-lg">
          <div className="panel-grid">
            <SkeletonPanelHeaderCard titleWidth="w-24" bodyLines={2} />
            <SkeletonPanelHeaderCard summaryWidth="w-28" titleWidth="w-40" bodyLines={0} />
          </div>
          <SkeletonPanelHeaderCard summaryWidth="w-24" titleWidth="w-40" bodyLines={2} />
        </section>
      );
    case 'traffic':
      return (
        <div className="stack-lg">
          <SkeletonPanelHeaderCard summaryWidth="w-20" titleWidth="w-40" bodyLines={2} />
          <SkeletonPanelHeaderCard summaryWidth="w-20" titleWidth="w-40" bodyLines={2} />
        </div>
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

function AppMetric({ label, value }: { label: string; value: string }) {
  return (
    <li className="apps-metric">
      <span className="apps-metric-value">{value}</span>
      <span className="apps-metric-label">{label}</span>
    </li>
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

const tabOrder: AppTabId[] = [
  'deploy',
  'proxy',
  'traffic',
  'runtime',
  'config',
  'infra',
  'logs',
  'settings',
];

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
