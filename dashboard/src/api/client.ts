// The daemon's API adapted to the shapes the screens render.
//
// Every screen talks to `client`. Reads combine the endpoints a screen needs;
// writes call one endpoint and then notify subscribers, so the queries that
// show the change refresh at once. The daemon's own event stream does the same
// for changes made elsewhere: the CLI, git push, a deploy token, the watcher.

import type {
  AcmeSettings,
  AppDetail,
  AppSummary,
  AuthConfig,
  ConfigVar,
  DashboardToken,
  Deployment,
  DeployStep,
  DockerNetwork,
  DoctorCheck,
  Domain,
  EventRecord,
  HealthCheckResult,
  HostEvent,
  HostStatus,
  LogLine,
  ManagedService,
  ObjectStore,
  ProcessRecord,
  ProxyStatus,
  Redirect,
  Route,
  ServiceBackup,
  ServiceKind,
  SshKey,
  StepTiming,
} from '../lib/types';
import * as api from './daemon';
import { apiText, followStream, runConsole } from './http';

const KINDS: ServiceKind[] = ['postgres', 'mysql', 'mariadb', 'redis', 'mongodb'];
const IN_PROGRESS = new Set(['pending', 'building', 'built', 'deploying', 'health_checking']);

const enc = encodeURIComponent;

// ── Subscribers ───────────────────────────────────────────────────────────────

const listeners = new Set<(event: HostEvent) => void>();

function notify(event: HostEvent): void {
  for (const listener of listeners) listener(event);
}

/** App ids the event stream reports, mapped to the names queries are keyed by. */
const appNames = new Map<string, string>();

function remember(apps: api.App[]): void {
  for (const app of apps) appNames.set(app.id, app.name);
}

async function appId(name: string): Promise<string> {
  for (const [id, known] of appNames) if (known === name) return id;
  const app = await api.fetchApp(name);
  remember([app]);
  return app.id;
}

// ── Mapping helpers ───────────────────────────────────────────────────────────

function parsePayload(payload: string | null): Record<string, unknown> {
  if (!payload) return {};
  try {
    const parsed: unknown = JSON.parse(payload);
    return parsed && typeof parsed === 'object' ? (parsed as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

/**
 * Rebuild each deploy's steps from the app's events.
 *
 * Only `deploy.started` and the closing event name the deploy, but the daemon
 * runs one deploy per app at a time, so every event in between belongs to the
 * deploy that started last.
 */
function deriveSteps(
  events: api.EventRecord[]
): Map<string, { steps: Partial<Record<DeployStep, StepTiming>>; failed?: DeployStep }> {
  const result = new Map<
    string,
    { steps: Partial<Record<DeployStep, StepTiming>>; failed?: DeployStep }
  >();
  const ordered = [...events].sort((a, b) => a.created_at.localeCompare(b.created_at));
  let current: string | null = null;
  // Read through a function: TypeScript does not see `start` reassigning it.
  let open: DeployStep | null = null;
  const openStep = (): DeployStep | null => open;

  const start = (step: DeployStep, at: string) => {
    if (!current) return;
    const entry = result.get(current);
    if (!entry) return;
    if (open && entry.steps[open] && !entry.steps[open]?.finished_at) {
      (entry.steps[open] as StepTiming).finished_at = at;
    }
    entry.steps[step] = { started_at: at, finished_at: null };
    open = step;
  };

  for (const event of ordered) {
    const payload = parsePayload(event.payload);
    switch (event.event_type) {
      case 'deploy.started':
        current = typeof payload.deploy_id === 'string' ? payload.deploy_id : null;
        if (current) result.set(current, { steps: {} });
        open = null;
        start('build', event.created_at);
        break;
      case 'deploy.release':
        start('release', event.created_at);
        break;
      case 'deploy.health_checking':
        start('health_check', event.created_at);
        break;
      case 'deploy.live':
        start('live', event.created_at);
        if (current && openStep()) {
          const live = result.get(current)?.steps.live;
          if (live) live.finished_at = event.created_at;
        }
        break;
      case 'deploy.complete':
      case 'deploy.failed': {
        const entry = current ? result.get(current) : undefined;
        const step = openStep();
        if (entry && step) {
          const timing = entry.steps[step];
          if (timing && !timing.finished_at) timing.finished_at = event.created_at;
          if (event.event_type === 'deploy.failed') entry.failed = step;
        }
        current = null;
        open = null;
        break;
      }
      default:
        break;
    }
  }
  return result;
}

function toDeployment(
  raw: api.Deployment,
  derived?: { steps: Partial<Record<DeployStep, StepTiming>>; failed?: DeployStep }
): Deployment {
  return {
    id: raw.id,
    app_id: raw.app_id,
    status: raw.status,
    builder: raw.builder,
    image_tag: raw.image_tag,
    created_at: raw.created_at,
    finished_at: IN_PROGRESS.has(raw.status) ? null : raw.finished_at,
    failed_step: raw.status === 'failed' ? derived?.failed : undefined,
    steps: derived?.steps ?? {},
  };
}

/** The process type the daemon stores an app-wide limit under. */
const APP_WIDE_LIMIT = '_all_';

function toProcesses(
  scale: api.ScaleMap,
  all: api.ProcessRecord[],
  limits: api.AppLimit[],
  current: string | null
): ProcessRecord[] {
  // Retained deployments keep their containers running for their own URLs; only the
  // current release's containers count toward its health.
  const ps = current ? all.filter((p) => p.deployment_id === current) : all;
  const types = new Set<string>([...Object.keys(scale), ...ps.map((p) => p.process_type)]);
  return [...types].sort().map((type) => {
    const running = ps.filter((p) => p.process_type === type);
    const limit =
      limits.find((l) => l.process_type === type) ??
      limits.find((l) => l.process_type === APP_WIDE_LIMIT);
    return {
      process_type: type,
      scale: scale[type] ?? Math.max(running.length, 1),
      healthy: running.filter((p) => p.status === 'running').length,
      memory_limit: limit?.memory ?? null,
      cpu_limit: limit?.cpu ? Number(limit.cpu) : null,
    };
  });
}

function toDomain(raw: api.DomainWithTls): Domain {
  const tls = raw.tls;
  const state = tls?.state ?? 'pending';
  return {
    hostname: raw.domain,
    tls: state === 'active' || state === 'manual' ? 'active' : 'none',
    mode: tls?.mode ?? 'auto',
    state,
    details: tls?.details ?? undefined,
    expires_at: tls?.expires_at ?? undefined,
  };
}

function formatSize(bytes: number | null): string {
  if (bytes === null) return 'size unknown';
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

/** One line describing an event, from its type and payload. */
function describeEvent(type: string, payload: Record<string, unknown>): string {
  const text = (key: string) => (typeof payload[key] === 'string' ? (payload[key] as string) : '');
  switch (type) {
    case 'deploy.started':
      return 'Deploy started';
    case 'deploy.complete':
      return 'Deploy finished';
    case 'deploy.failed':
      return text('error') || text('reason') || 'Deploy failed';
    case 'deploy.live':
      return text('deployment_url') ? `Live at ${text('deployment_url')}` : 'Live';
    case 'deploy.rollback':
      return 'Rolled back';
    case 'certificate.issued':
      return `${text('domain')} is served over HTTPS`;
    case 'certificate.failed':
      return `${text('domain')}: certificate requests paused. ${text('details')}`.trim();
    case 'app.created':
      return 'App created';
    case 'app.deleted':
      return 'App destroyed';
    default:
      return text('message') || text('line') || type;
  }
}

function toEvent(raw: api.EventRecord): EventRecord {
  const payload = parsePayload(raw.payload);
  return {
    id: raw.id,
    app: raw.app_id ? (appNames.get(raw.app_id) ?? null) : null,
    kind: raw.event_type,
    message: describeEvent(raw.event_type, payload),
    created_at: raw.created_at,
  };
}

function toLogLine(app: string, raw: api.LogLine): LogLine {
  return {
    id: raw.id,
    app,
    deployment_id: raw.deployment_id,
    source: raw.source === 'build' ? 'build' : 'runtime',
    stream: raw.stream === 'stderr' ? 'stderr' : 'stdout',
    process: raw.source,
    message: raw.message,
    created_at: raw.created_at,
  };
}

/** Split a command line on spaces, keeping quoted parts together. */
function splitCommand(command: string): string[] {
  const parts: string[] = [];
  const pattern = /"([^"]*)"|'([^']*)'|(\S+)/g;
  for (let match = pattern.exec(command); match; match = pattern.exec(command)) {
    parts.push(match[1] ?? match[2] ?? match[3]);
  }
  return parts;
}

// ── Services ──────────────────────────────────────────────────────────────────

async function serviceDetails(): Promise<
  Array<{ kind: ServiceKind; detail: api.ManagedServiceDetail }>
> {
  const lists = await Promise.all(
    KINDS.map(async (kind) => ({
      kind,
      services: await api.fetchManagedServices(kind).catch(() => []),
    }))
  );
  const details = await Promise.all(
    lists.flatMap(({ kind, services }) =>
      services.map(async (service) => ({
        kind,
        detail: await api.fetchManagedService(kind, service.name),
      }))
    )
  );
  return details;
}

async function kindOf(name: string): Promise<ServiceKind> {
  const found = (await serviceDetails()).find(({ detail }) => detail.name === name);
  if (!found) throw new Error(`No service named ${name}`);
  return found.kind;
}

// ── Event stream ──────────────────────────────────────────────────────────────
//
// A tab keeps one connection to the daemon. Over plain HTTP a browser allows
// only a few connections per host, and a second long-lived stream plus a
// running command or health check would leave every other request waiting.
// The stream carries the host's events and, when a screen shows logs, that
// app's lines too. It closes while the tab is hidden, so background tabs hold
// nothing, and a reconnect after a gap refreshes what the gap may have missed.

type LogSubscription = {
  app: string;
  deployment: string | null;
  onLine: (line: LogLine) => void;
};

const logSubscriptions: LogSubscription[] = [];
let connection: { path: string; stop: () => void } | undefined;
let live = false;
let missedEvents = false;
const liveWatchers = new Set<() => void>();

function setLive(next: boolean): void {
  if (live === next) return;
  live = next;
  for (const watcher of liveWatchers) watcher();
}

/** The screen shown last decides whose log lines the stream carries. */
function streamPath(logs: LogSubscription | undefined): string {
  if (!logs) return '/api/events/stream';
  const params = new URLSearchParams({ logs: logs.app });
  if (logs.deployment) {
    params.set('deployment', logs.deployment);
    params.set('source', 'build');
  } else {
    params.set('source', 'runtime');
  }
  return `/api/events/stream?${params}`;
}

function onFrame(data: string, event: string | undefined): void {
  try {
    if (event === 'log') {
      const logs = logSubscriptions.at(-1);
      if (!logs) return;
      const line = toLogLine(logs.app, JSON.parse(data) as api.LogLine);
      for (const subscription of logSubscriptions) {
        if (subscription.app === logs.app && subscription.deployment === logs.deployment) {
          subscription.onLine(line);
        }
      }
      return;
    }
    const raw = JSON.parse(data) as api.EventRecord;
    const app = raw.app_id ? appNames.get(raw.app_id) : undefined;
    notify({ type: 'event', event: toEvent(raw) });
    if (raw.event_type.startsWith('deploy.') || raw.event_type.startsWith('certificate.')) {
      notify(app ? { type: 'app.changed', app } : { type: 'settings.changed' });
    } else if (raw.event_type.startsWith('app.')) {
      notify({ type: 'settings.changed' });
    }
  } catch {
    // An unreadable frame is skipped; the next one still arrives.
  }
}

/** Open, switch, or close the stream to match the listeners and the tab. */
function reconcileStream(): void {
  const visible = typeof document === 'undefined' || document.visibilityState !== 'hidden';
  const wanted = listeners.size > 0 && visible;
  const path = streamPath(logSubscriptions.at(-1));

  if (connection && (!wanted || connection.path !== path)) {
    connection.stop();
    connection = undefined;
    // Switching screens reconnects within milliseconds; a hidden tab may miss
    // anything.
    if (!wanted) {
      missedEvents = true;
      setLive(false);
    }
  }
  if (!wanted || connection) return;

  connection = {
    path,
    stop: followStream(path, onFrame, (connected) => {
      setLive(connected);
      if (!connected) {
        missedEvents = true;
      } else if (missedEvents) {
        missedEvents = false;
        notify({ type: 'resync' });
      }
    }),
  };
}

if (typeof document !== 'undefined') {
  document.addEventListener('visibilitychange', reconcileStream);
}

// ── Client ────────────────────────────────────────────────────────────────────

export const client = {
  /** Follow the daemon's events, and changes made through this client. */
  subscribe(listener: (event: HostEvent) => void): () => void {
    listeners.add(listener);
    reconcileStream();
    return () => {
      listeners.delete(listener);
      reconcileStream();
    };
  },

  /**
   * Follow an app's log lines, or one deploy's build output. The lines ride on
   * the tab's one event stream rather than a connection of their own.
   */
  streamLogs(app: string, deployment: string | null, onLine: (line: LogLine) => void): () => void {
    const subscription: LogSubscription = { app, deployment, onLine };
    logSubscriptions.push(subscription);
    reconcileStream();
    return () => {
      logSubscriptions.splice(logSubscriptions.indexOf(subscription), 1);
      reconcileStream();
    };
  },

  /** Whether the event stream is connected now. */
  isLive(): boolean {
    return live;
  },

  /** Call `onChange` whenever the event stream connects or drops. */
  watchLive(onChange: () => void): () => void {
    liveWatchers.add(onChange);
    return () => liveWatchers.delete(onChange);
  },

  // ── Apps ──

  async listApps(): Promise<AppSummary[]> {
    const apps = await api.fetchApps();
    remember(apps);
    return Promise.all(
      apps.map(async (app) => {
        const [deployments, domains, ps, scale] = await Promise.all([
          api.fetchDeployments(app.name).catch(() => []),
          api.fetchDomains(app.name).catch(() => []),
          api.fetchProcesses(app.name).catch(() => []),
          api.fetchScale(app.name).catch(() => ({})),
        ]);
        const mapped = deployments.map((d) => toDeployment(d));
        const current = mapped.find((d) => d.status === 'live') ?? null;
        return {
          app,
          domain: domains[0]?.domain ?? null,
          processes: toProcesses(scale, ps, [], current?.id ?? null),
          current,
          active: mapped.find((d) => IN_PROGRESS.has(d.status)) ?? null,
          recent: mapped.slice(0, 12).map((d) => d.status),
        };
      })
    );
  },

  async createApp(name: string): Promise<AppDetail> {
    await api.createApp(name);
    notify({ type: 'settings.changed' });
    return client.getApp(name);
  },

  async getApp(name: string): Promise<AppDetail> {
    const app = await api.fetchApp(name);
    remember([app]);
    const [
      domains,
      ps,
      scale,
      limits,
      maintenance,
      ports,
      redirects,
      auth,
      environments,
      tokens,
      networks,
      mounts,
      cron,
      objectStore,
      certificate,
      services,
      routing,
      deployments,
    ] = await Promise.all([
      api.fetchDomainsWithTls(name),
      api.fetchProcesses(name).catch(() => []),
      api.fetchScale(name).catch(() => ({})),
      api.fetchAppLimits(name).catch(() => []),
      api.fetchMaintenance(name).catch(() => ({ enabled: false, message: null })),
      api.fetchPorts(name).catch(() => []),
      api.fetchRedirects(name).catch(() => []),
      api.fetchAppAuth(name).catch(() => null),
      api.fetchEnvironments(name).catch(() => []),
      api.fetchDeployTokens(name).catch(() => []),
      api.fetchAppNetworks(name).catch(() => []),
      api.fetchStorageMounts(name).catch(() => []),
      api.fetchCronEntries(name).catch(() => []),
      api.fetchAppObjectStoreLink(name).catch(() => null),
      api.fetchTlsStatus(name).catch(() => null),
      serviceDetails().catch(() => []),
      api.fetchRoutingTable().catch(() => []),
      api.fetchDeployments(name).catch(() => []),
    ]);
    const route = routing.find((entry) => entry.app === name);
    const authConfig: AuthConfig =
      auth?.configured && auth.mode === 'basic'
        ? { mode: 'basic', username: auth.username ?? '' }
        : auth?.configured && auth.mode === 'forward'
          ? { mode: 'forward', url: auth.forward_url ?? '' }
          : { mode: 'none' };
    const live = deployments.find((d) => d.status === 'live');

    return {
      app,
      domains: domains.map(toDomain),
      processes: toProcesses(scale, ps, limits, live?.id ?? null),
      linked_services: services
        .filter(({ detail }) => detail.links.some((link) => link.name === name))
        .map(({ detail }) => detail.name),
      maintenance: maintenance.enabled,
      ports: ports.map((p) => ({
        id: p.id,
        host_port: p.host_port,
        container_port: p.container_port,
      })),
      redirects: redirects.map((r) => ({
        id: r.id,
        from: r.source_path,
        to: r.target,
        code: (r.code as Redirect['code']) ?? 302,
      })),
      auth: authConfig,
      environments: environments.map((env) => ({
        slug: env.slug,
        name: env.name,
        hostname: env.is_production
          ? (domains[0]?.domain ??
            route?.hostnames.find((h) => h.environment === env.slug && !h.deployment_id)
              ?.hostname ??
            null)
          : (route?.hostnames.find((h) => h.environment === env.slug && !h.deployment_id)
              ?.hostname ?? null),
        live_release: env.is_production && live ? live.id.slice(0, 7) : null,
      })),
      tokens,
      networks: networks.map((network) => network.name),
      mounts: mounts.map((m) => ({
        id: m.id,
        host_path: m.host_path,
        container_path: m.container_path,
      })),
      cron,
      object_store:
        objectStore?.linked && objectStore.link
          ? { bucket: objectStore.link.bucket, prefix: objectStore.link.prefix }
          : null,
      certificate: {
        path: certificate?.certificate.path ?? `/etc/angie/ssl/deku_${name}.crt`,
        present: Boolean(certificate?.certificate.exists),
        expires_at: certificate?.expires_at ?? null,
      },
    };
  },

  // ── Deploys ──

  async listDeployments(name: string): Promise<Deployment[]> {
    const [deployments, events] = await Promise.all([
      api.fetchDeployments(name),
      appId(name)
        .then((id) => api.fetchEvents(id))
        .catch(() => []),
    ]);
    const derived = deriveSteps(events);
    return deployments.map((d) => toDeployment(d, derived.get(d.id)));
  },

  async getDeployment(name: string, id: string): Promise<Deployment> {
    const found = (await client.listDeployments(name)).find((d) => d.id === id);
    if (!found) throw new Error(`No deploy ${id} for ${name}`);
    return found;
  },

  async deploymentLogs(name: string, id: string): Promise<LogLine[]> {
    const lines = await api.fetchLogs(name, 1000, { deployment: id, source: 'build' });
    return lines.map((line) => toLogLine(name, line));
  },

  async recentLogs(name: string): Promise<LogLine[]> {
    const lines = await api.fetchLogs(name, 400, { source: 'runtime' });
    return lines.map((line) => toLogLine(name, line));
  },

  /** Deploy `image`, or redeploy the image the app is serving. */
  async startDeploy(name: string, image?: string): Promise<Deployment> {
    const before = new Set((await api.fetchDeployments(name)).map((d) => d.id));
    let reference = image;
    if (!reference) {
      const live = (await api.fetchDeployments(name)).find((d) => d.status === 'live');
      if (!live?.image_tag)
        throw new Error(`${name} has nothing deployed to redeploy yet. Deploy an image first.`);
      reference = live.image_tag;
    }
    await api.triggerImageDeploy(name, reference);
    return client.newestDeployment(name, before);
  },

  async uploadArchive(name: string, file: File): Promise<Deployment> {
    const before = new Set((await api.fetchDeployments(name)).map((d) => d.id));
    await api.triggerArchiveDeploy(name, file);
    return client.newestDeployment(name, before);
  },

  async rollback(name: string): Promise<Deployment> {
    const deployments = await api.fetchDeployments(name);
    const live = deployments.filter((d) => d.status === 'live');
    if (live.length < 2) throw new Error('There is no earlier release to roll back to');
    const before = new Set(deployments.map((d) => d.id));
    await api.triggerRollback(name, live[1].id);
    return client.newestDeployment(name, before);
  },

  /** The deployment a request just started: the first one not in `before`. */
  async newestDeployment(name: string, before: Set<string>): Promise<Deployment> {
    for (let attempt = 0; attempt < 20; attempt += 1) {
      const fresh = (await api.fetchDeployments(name)).find((d) => !before.has(d.id));
      if (fresh) {
        notify({ type: 'app.changed', app: name });
        return toDeployment(fresh);
      }
      await new Promise((resolve) => window.setTimeout(resolve, 250));
    }
    throw new Error('The deploy was accepted but has not appeared yet. Check Deploys in a moment.');
  },

  // ── Processes and traffic ──

  async setScale(name: string, process: string, scale: number): Promise<AppDetail> {
    await api.setScale(name, { [process]: scale });
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async setLimits(
    name: string,
    process: string,
    memory: string | null,
    cpu: number | null
  ): Promise<AppDetail> {
    await api.setAppLimit(name, {
      process_type: process,
      memory,
      cpu: cpu === null ? null : String(cpu),
    });
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async setMaintenance(name: string, on: boolean): Promise<AppDetail> {
    await api.setMaintenance(name, { enabled: on, message: null });
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async addRedirect(
    name: string,
    from: string,
    to: string,
    code: Redirect['code']
  ): Promise<AppDetail> {
    await api.addRedirect(name, { source_path: from, target: to, code });
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async removeRedirect(name: string, id: string): Promise<AppDetail> {
    await api.removeRedirect(name, id);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async setAuth(name: string, auth: AuthConfig): Promise<AppDetail> {
    if (auth.mode === 'none') {
      await api.clearAppAuth(name);
    } else if (auth.mode === 'basic') {
      if (!auth.password) throw new Error('Enter a password for the username');
      await api.setAppAuth(name, {
        mode: 'basic',
        username: auth.username,
        password: auth.password,
      });
    } else {
      await api.setAppAuth(name, { mode: 'forward', forward_url: auth.url });
    }
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  // ── Domains and ports ──

  async addDomain(name: string, hostname: string): Promise<AppDetail> {
    await api.addDomain(name, hostname.trim().toLowerCase());
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async removeDomain(name: string, hostname: string): Promise<AppDetail> {
    await api.removeDomain(name, hostname);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async setDomainMode(name: string, hostname: string, mode: Domain['mode']): Promise<AppDetail> {
    await api.setDomainTls(name, hostname, mode);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async retryDomain(name: string, hostname: string): Promise<AppDetail> {
    await api.retryDomainTls(name, hostname);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async addPort(name: string, hostPort: number, containerPort: number): Promise<AppDetail> {
    await api.addPortMapping(name, hostPort, containerPort);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async removePort(name: string, id: string): Promise<AppDetail> {
    await api.removePortMapping(name, id);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  // ── Environments, tokens, storage, networks, jobs ──

  async addEnvironment(name: string, slug: string): Promise<AppDetail> {
    await api.createEnvironment(name, { name: slug, slug });
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async removeEnvironment(name: string, slug: string): Promise<AppDetail> {
    await api.deleteEnvironment(name, slug);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async createToken(
    name: string,
    tokenName: string
  ): Promise<{ detail: AppDetail; secret: string }> {
    const created = await api.createDeployToken(name, tokenName.trim());
    notify({ type: 'app.changed', app: name });
    return { detail: await client.getApp(name), secret: created.token };
  },

  async revokeToken(name: string, id: string): Promise<AppDetail> {
    await api.revokeDeployToken(name, id);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async attachNetwork(name: string, network: string): Promise<AppDetail> {
    await api.attachAppNetwork(name, network);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async detachNetwork(name: string, network: string): Promise<AppDetail> {
    await api.detachAppNetwork(name, network);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async addMount(name: string, hostPath: string, containerPath: string): Promise<AppDetail> {
    await api.addStorageMount(name, hostPath, containerPath);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async removeMount(name: string, id: string): Promise<AppDetail> {
    await api.removeStorageMount(name, id);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async addCron(name: string, schedule: string, command: string): Promise<AppDetail> {
    await api.addCronEntry(name, schedule.trim(), command.trim());
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async removeCron(name: string, id: string): Promise<AppDetail> {
    await api.removeCronEntry(name, id);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async linkObjectStore(name: string, prefix: string): Promise<AppDetail> {
    await api.linkAppObjectStore(name, prefix);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  async unlinkObjectStore(name: string): Promise<AppDetail> {
    await api.unlinkAppObjectStore(name);
    notify({ type: 'app.changed', app: name });
    return client.getApp(name);
  },

  // ── Config ──

  /** App-wide values, every environment's overrides, and what services set. */
  async listConfig(name: string): Promise<ConfigVar[]> {
    const [appWide, environments, services, objectStore] = await Promise.all([
      api.fetchConfig(name),
      api.fetchEnvironments(name).catch(() => []),
      serviceDetails().catch(() => []),
      api.fetchAppObjectStoreLink(name).catch(() => null),
    ]);
    const serviceKeys = new Map<string, string>();
    for (const { detail } of services) {
      for (const link of detail.links)
        if (link.name === name) serviceKeys.set(link.env_key, detail.name);
    }
    const objectKeys = new Set(objectStore?.link?.linked_keys ?? []);
    const owner = (key: string) =>
      serviceKeys.get(key) ?? (objectKeys.has(key) ? 'object store' : undefined);

    const vars: ConfigVar[] = appWide.map((v) => ({
      key: v.key,
      value: v.value,
      service: owner(v.key),
    }));
    const overrides = await Promise.all(
      environments
        .filter((env) => !env.is_production)
        .map(async (env) => ({
          env,
          values: await api.fetchConfig(name, env.slug).catch(() => []),
        }))
    );
    for (const { env, values } of overrides) {
      for (const v of values.filter((value) => value.source === 'environment')) {
        vars.push({ key: v.key, value: v.value, environment: env.slug });
      }
    }
    return vars;
  },

  async setEnvConfig(
    name: string,
    environment: string | undefined,
    key: string,
    value: string
  ): Promise<ConfigVar[]> {
    await api.setConfigVar(name, key, value, environment);
    notify({ type: 'app.changed', app: name });
    return client.listConfig(name);
  },

  async unsetEnvConfig(
    name: string,
    environment: string | undefined,
    key: string
  ): Promise<ConfigVar[]> {
    await api.deleteConfigVar(name, key, environment);
    notify({ type: 'app.changed', app: name });
    return client.listConfig(name);
  },

  // ── Runtime ──

  async runHealthCheck(name: string): Promise<HealthCheckResult[]> {
    const result = await api.runAppChecks(name);
    const probes = result.probes.length > 0 ? result.probes : result.probe ? [result.probe] : [];
    if (probes.length === 0)
      throw new Error(result.issues[0] ?? 'There is no running web process to check');
    return probes.map((probe) => ({
      container: probe.target,
      status: probe.ok ? 'pass' : 'fail',
      latency_ms: probe.latency_ms,
      detail: probe.error ?? `GET ${probe.path} returned ${probe.status_code ?? 'no response'}`,
    }));
  },

  async runCommand(
    name: string,
    command: string,
    mode: 'fresh' | 'live'
  ): Promise<{ exit_code: number; output: string[] }> {
    const words = splitCommand(command);
    if (words.length === 0) throw new Error('Enter a command');
    const result = await runConsole(
      `/api/apps/${enc(name)}/${mode === 'fresh' ? 'run' : 'exec'}`,
      words
    );
    return { exit_code: result.exitCode, output: result.output };
  },

  // ── Host and settings ──

  async getToken(): Promise<DashboardToken> {
    const session = await api.fetchSession();
    return { expires_at: session.token_expires_at };
  },

  async rotateToken(): Promise<{ token: DashboardToken; secret: string }> {
    const { token } = await api.rotateDashboardToken();
    notify({ type: 'settings.changed' });
    return { token: await client.getToken(), secret: token };
  },

  async getAcme(): Promise<AcmeSettings> {
    const [status, email] = await Promise.all([
      api.fetchAcmeStatus(),
      api.fetchLetsEncryptConfig().catch(() => ({ configured: false, email: null })),
    ]);
    const lifecycle = status.certificate.lifecycle;
    return {
      enabled: status.enabled,
      custom_domains: status.custom_domains,
      email: email.email ?? status.account_email,
      global_domain: status.global_domain,
      directory: status.directory,
      provider: status.provider,
      wildcard: {
        enabled: status.enabled && status.wildcard,
        token_set: status.token_configured,
        status: !status.wildcard
          ? 'off'
          : lifecycle === 'ok' || lifecycle === 'expiring'
            ? 'issued'
            : 'pending',
      },
    };
  },

  async setAcme(next: {
    custom_domains?: boolean;
    wildcard?: boolean;
    email?: string | null;
    /** The ACME directory: Let's Encrypt's production or staging URL. */
    directory?: string;
  }): Promise<AcmeSettings> {
    if (next.email !== undefined && next.email) await api.setLetsEncryptConfig(next.email);
    if (
      next.custom_domains !== undefined ||
      next.wildcard !== undefined ||
      next.directory !== undefined
    ) {
      const current = await client.getAcme();
      const customDomains = next.custom_domains ?? current.custom_domains;
      const wildcard = next.wildcard ?? current.wildcard.enabled;
      await api.saveAcmeSettings({
        enabled: customDomains || wildcard,
        directory: next.directory ?? current.directory,
        provider: current.provider,
        wildcard,
        custom_domains: customDomains,
      });
    }
    notify({ type: 'settings.changed' });
    return client.getAcme();
  },

  async setDnsToken(token: string): Promise<AcmeSettings> {
    const current = await client.getAcme();
    await api.verifyAcmeToken(token);
    await api.saveAcmeSettings({
      enabled: true,
      directory: current.directory,
      provider: current.provider,
      wildcard: true,
      custom_domains: current.custom_domains,
      api_token: token,
    });
    notify({ type: 'settings.changed' });
    return client.getAcme();
  },

  async getObjectStore(): Promise<ObjectStore | null> {
    const state = await api.fetchObjectStoreConfig();
    const store = state.object_store;
    return state.configured && store
      ? {
          provider: store.provider,
          endpoint: store.endpoint,
          bucket: store.bucket,
          region: store.region,
          access_key_id: store.access_key_id,
          path_style: store.path_style,
        }
      : null;
  },

  async setObjectStore(next: ObjectStore, secret: string): Promise<ObjectStore> {
    if (!secret) throw new Error('Enter the secret access key');
    await api.setObjectStoreConfig({
      provider: next.provider,
      endpoint: next.endpoint,
      bucket: next.bucket,
      region: next.region,
      access_key_id: next.access_key_id,
      secret_access_key: secret,
      path_style: next.path_style,
    });
    notify({ type: 'settings.changed' });
    const saved = await client.getObjectStore();
    if (!saved) throw new Error('The object store was not saved');
    return saved;
  },

  async testObjectStore(): Promise<{ ok: boolean; detail: string }> {
    const result = await api.testObjectStoreConfig();
    return {
      ok: result.ok,
      detail: result.ok ? 'Connected: wrote and read a test object.' : 'The test did not pass.',
    };
  },

  async removeObjectStore(): Promise<null> {
    await api.unsetObjectStoreConfig();
    notify({ type: 'settings.changed' });
    return null;
  },

  async listSshKeys(): Promise<SshKey[]> {
    return (await api.fetchSshKeys()).map((key) => ({
      name: key.name,
      fingerprint: key.fingerprint,
    }));
  },

  async addSshKey(name: string, publicKey: string): Promise<SshKey[]> {
    await api.addSshKey(name.trim(), publicKey.trim());
    notify({ type: 'settings.changed' });
    return client.listSshKeys();
  },

  async removeSshKey(name: string): Promise<SshKey[]> {
    await api.deleteSshKey(name);
    notify({ type: 'settings.changed' });
    return client.listSshKeys();
  },

  async listNetworks(): Promise<DockerNetwork[]> {
    return (await api.fetchNetworks()).map((network) => ({ name: network.name }));
  },

  async createNetwork(name: string): Promise<DockerNetwork[]> {
    await api.createNetwork(name);
    notify({ type: 'settings.changed' });
    return client.listNetworks();
  },

  async removeNetwork(name: string): Promise<DockerNetwork[]> {
    await api.deleteNetwork(name);
    notify({ type: 'settings.changed' });
    return client.listNetworks();
  },

  async listRoutes(): Promise<Route[]> {
    const table = await api.fetchRoutingTable();
    const routes: Route[] = [];
    for (const entry of table) {
      const domains = await api.fetchDomainsWithTls(entry.app).catch(() => []);
      const upstreams = entry.upstreams.map((u) => `${u.host}:${u.port}`);
      for (const domain of domains) {
        const state = domain.tls?.state;
        routes.push({
          hostname: domain.domain,
          app: entry.app,
          environment: 'production',
          upstreams,
          tls: state === 'active' || state === 'manual' ? 'active' : 'none',
        });
      }
      for (const host of entry.hostnames.filter(
        (h) => !h.deployment_id && !entry.domains.includes(h.hostname)
      )) {
        routes.push({
          hostname: host.hostname,
          app: entry.app,
          environment: host.environment,
          upstreams,
          tls: 'none',
        });
      }
    }
    return routes;
  },

  async getProxy(): Promise<ProxyStatus> {
    const status = await api.fetchRoutingStatus();
    return { valid: status.angie.config_valid, error: status.angie.validation_error ?? null };
  },

  // ── Services ──

  async listServices(): Promise<ManagedService[]> {
    const [details, schedules] = await Promise.all([
      serviceDetails(),
      api.fetchBackupSchedules().catch(() => []),
    ]);
    return Promise.all(
      details.map(async ({ kind, detail }) => {
        const backups = await api.fetchServiceBackups(kind, detail.name).catch(() => []);
        const schedule = schedules.find((s) => s.service === detail.name && s.enabled);
        return {
          name: detail.name,
          kind,
          status: detail.status,
          linked_apps: detail.links.map((link) => link.name),
          last_backup_at: backups[0]?.created_at ?? null,
          backup_schedule: schedule
            ? { interval_hours: schedule.interval_hours ?? 24, keep: schedule.retention ?? 7 }
            : null,
        };
      })
    );
  },

  async createService(kind: ServiceKind, name: string): Promise<ManagedService> {
    await api.createManagedService(kind, name);
    notify({ type: 'services.changed' });
    const created = (await client.listServices()).find((service) => service.name === name);
    if (!created) throw new Error(`${name} was created but is not listed yet`);
    return created;
  },

  async linkService(service: string, app: string): Promise<ManagedService> {
    await api.linkManagedService(await kindOf(service), service, app);
    notify({ type: 'services.changed' });
    notify({ type: 'app.changed', app });
    return (await client.listServices()).find((entry) => entry.name === service) as ManagedService;
  },

  async unlinkService(service: string, app: string): Promise<ManagedService> {
    await api.unlinkManagedService(await kindOf(service), service, app);
    notify({ type: 'services.changed' });
    notify({ type: 'app.changed', app });
    return (await client.listServices()).find((entry) => entry.name === service) as ManagedService;
  },

  async serviceConnection(name: string): Promise<{ url: string; host: string; port: number }> {
    const detail = await api.fetchManagedService(await kindOf(name), name);
    const connection = detail.connection;
    return {
      url: String(connection.url ?? connection.uri ?? ''),
      host: String(connection.host ?? ''),
      port: Number(connection.port ?? 0),
    };
  },

  async serviceLogs(name: string): Promise<string[]> {
    return api.fetchManagedServiceLogs(await kindOf(name), name, 50);
  },

  async listBackups(name: string): Promise<ServiceBackup[]> {
    const backups = await api.fetchServiceBackups(await kindOf(name), name);
    return backups.map((backup) => ({
      id: backup.id,
      created_at: backup.created_at,
      size: formatSize(backup.size_bytes),
    }));
  },

  async backupService(name: string): Promise<ManagedService> {
    await api.triggerServiceBackup(await kindOf(name), name);
    notify({ type: 'services.changed' });
    return (await client.listServices()).find((entry) => entry.name === name) as ManagedService;
  },

  async restoreBackup(name: string, id: string): Promise<null> {
    await api.restoreServiceBackup(await kindOf(name), name, id);
    notify({ type: 'services.changed' });
    return null;
  },

  async setBackupSchedule(
    name: string,
    schedule: ManagedService['backup_schedule']
  ): Promise<ManagedService> {
    if (schedule) {
      await api.setServiceBackupSchedule(name, {
        interval_hours: schedule.interval_hours,
        retention: schedule.keep,
      });
    } else {
      await api.deleteServiceBackupSchedule(name);
    }
    notify({ type: 'services.changed' });
    return (await client.listServices()).find((entry) => entry.name === name) as ManagedService;
  },

  // ── Host ──

  async listChecks(): Promise<DoctorCheck[]> {
    const doctor = await api.fetchDoctor();
    return doctor.checks.map((check) => ({
      id: check.name,
      name: check.name.replaceAll('_', ' '),
      status: check.status === 'ok' ? 'pass' : check.status === 'fail' ? 'fail' : 'warn',
      detail: check.detail,
    }));
  },

  async getHost(): Promise<HostStatus> {
    const [version, metrics] = await Promise.all([
      api.fetchVersionStatus().catch(() => null),
      apiText('/api/metrics').catch(() => ''),
    ]);
    const parsed = api.parseMetrics(metrics);
    const total = (name: string) =>
      (parsed.get(name) ?? []).reduce((sum, sample) => sum + sample.value, 0);
    return {
      hostname: window.location.hostname,
      // Shown with a leading v wherever it appears, whichever form the daemon reports.
      version: version ? `v${version.current_version.replace(/^v/, '')}` : 'unknown',
      latest_version: version?.latest_version
        ? `v${version.latest_version.replace(/^v/, '')}`
        : null,
      update_available: version?.update_available ?? false,
      counts: {
        apps: total('deku_apps_total'),
        deployed: total('deku_apps_deployed'),
        containers: total('deku_containers_running'),
        services: total('deku_services_total'),
      },
    };
  },

  async listEvents(): Promise<EventRecord[]> {
    if (appNames.size === 0) remember(await api.fetchApps().catch(() => []));
    return (await api.fetchEvents()).slice(0, 40).map(toEvent);
  },

  // ── Session ──

  signIn: api.signIn,
  signOut: api.signOut,
  session: api.fetchSession,
};
