// An in-memory stand-in for the daemon. It answers the same questions the HTTP API does and pushes
// the same kind of events the host stream will, so the screens can be built and judged before the
// real client is wired in.

import type {
  AcmeSettings,
  AppDetail,
  AppSummary,
  AuthConfig,
  ConfigVar,
  DashboardToken,
  Deployment,
  DeployStatus,
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
  ProxyStatus,
  Redirect,
  Route,
  ServiceBackup,
  ServiceKind,
  SshKey,
} from '../lib/types';
import { extras, type MockStore, STEP_ORDER, seed, shortId } from './data';

const store: MockStore = seed();
const listeners = new Set<(event: HostEvent) => void>();
const logs = new Map<string, LogLine[]>();
const MAX_LOG_LINES = 2000;

function emit(event: HostEvent): void {
  for (const listener of listeners) listener(event);
}

function respond<T>(value: T, latency = 140): Promise<T> {
  return new Promise((resolve) => {
    window.setTimeout(() => resolve(structuredClone(value)), latency);
  });
}

function fail(message: string): Promise<never> {
  return new Promise((_, reject) => {
    window.setTimeout(() => reject(new Error(message)), 120);
  });
}

function findApp(name: string): AppDetail | undefined {
  return store.apps.find((entry) => entry.app.name === name);
}

/** Applies a change to one app and tells subscribers; a thrown Error becomes a rejected promise. */
function changeApp(name: string, change: (entry: AppDetail) => void): Promise<AppDetail> {
  const entry = findApp(name);
  if (!entry) return fail(`No app named ${name}`);
  try {
    change(entry);
  } catch (error) {
    return fail(error instanceof Error ? error.message : String(error));
  }
  emit({ type: 'app.changed', app: name });
  return respond(entry);
}

function deploymentsOf(name: string): Deployment[] {
  store.deployments[name] ??= [];
  return store.deployments[name];
}

const IN_PROGRESS: DeployStatus[] = [
  'pending',
  'building',
  'built',
  'deploying',
  'health_checking',
];

/** The daemon leaves every successful deploy as `live`; the newest one is what is serving. */
export function currentOf(deployments: Deployment[]): Deployment | null {
  return deployments.find((deployment) => deployment.status === 'live') ?? null;
}

function activeOf(deployments: Deployment[]): Deployment | null {
  return deployments.find((deployment) => IN_PROGRESS.includes(deployment.status)) ?? null;
}

// ── Logs ──────────────────────────────────────────────────────────────────────

function pushLog(line: Omit<LogLine, 'id' | 'created_at'>): void {
  const full: LogLine = {
    ...line,
    id: shortId() + Math.random().toString(16).slice(2, 6),
    created_at: new Date().toISOString(),
  };
  const lines = logs.get(line.app) ?? [];
  lines.push(full);
  if (lines.length > MAX_LOG_LINES) lines.splice(0, lines.length - MAX_LOG_LINES);
  logs.set(line.app, lines);
  emit({ type: 'log', line: full });
}

const RUNTIME_LINES: Record<string, Array<[string, LogLine['stream'], string]>> = {
  shop: [
    ['web', 'stdout', 'GET /products 200 in 34ms'],
    ['web', 'stdout', 'GET /cart 200 in 12ms'],
    ['web', 'stdout', 'POST /checkout/session 201 in 188ms'],
    ['web', 'stdout', 'GET /api/inventory?sku=TS-104 200 in 9ms'],
    ['worker', 'stdout', 'job email.order_confirmation done in 412ms'],
    ['worker', 'stdout', 'job inventory.sync done in 1.9s (318 rows)'],
    ['web', 'stderr', 'warn: slow query products.search 842ms'],
    ['web', 'stdout', 'GET /healthz 200 in 1ms'],
  ],
  api: [
    ['web', 'stdout', 'INFO request{method=GET path=/v1/orders} status=200 latency=18ms'],
    ['web', 'stdout', 'INFO request{method=POST path=/v1/orders} status=201 latency=61ms'],
    ['web', 'stdout', 'INFO request{method=GET path=/v1/customers/c_91f} status=200 latency=7ms'],
    ['web', 'stderr', 'WARN pool: connection acquired after 210ms'],
    ['web', 'stdout', 'INFO request{method=GET path=/healthz} status=200 latency=0ms'],
  ],
  billing: [
    ['web', 'stdout', '[INFO] Invoice run queued for 1,284 accounts'],
    ['scheduler', 'stdout', '[INFO] cron reconcile_payments started'],
    ['scheduler', 'stdout', '[INFO] cron reconcile_payments finished in 3.2s'],
    ['web', 'stdout', '[INFO] GET /invoices/2026-10 200 24ms'],
  ],
  docs: [
    ['web', 'stdout', '10.0.2.1 - "GET /installation/ HTTP/1.1" 200 18342'],
    ['web', 'stdout', '10.0.2.1 - "GET /assets/app.css HTTP/1.1" 304 0'],
  ],
};

function seedLogs(): void {
  const now = Date.now();
  for (const [app, samples] of Object.entries(RUNTIME_LINES)) {
    const current = currentOf(deploymentsOf(app));
    const lines: LogLine[] = [];
    for (let index = 0; index < 160; index += 1) {
      const [process, stream, message] = samples[(index * 7 + app.length) % samples.length];
      lines.push({
        id: `${app}-seed-${index}`,
        app,
        deployment_id: current?.id ?? null,
        source: 'runtime',
        stream,
        process: `${process}.${(index % 2) + 1}`,
        message,
        created_at: new Date(now - (160 - index) * 9000).toISOString(),
      });
    }
    logs.set(app, lines);
  }
}

seedLogs();

window.setInterval(() => {
  const running = store.apps.filter(
    (entry) =>
      entry.app.status === 'deployed' && !entry.maintenance && RUNTIME_LINES[entry.app.name]
  );
  if (running.length === 0) return;
  const entry = running[Math.floor(Math.random() * running.length)];
  const samples = RUNTIME_LINES[entry.app.name];
  const [process, stream, message] = samples[Math.floor(Math.random() * samples.length)];
  const replicas = entry.processes.find((p) => p.process_type === process)?.scale ?? 1;
  pushLog({
    app: entry.app.name,
    deployment_id: currentOf(deploymentsOf(entry.app.name))?.id ?? null,
    source: 'runtime',
    stream,
    process: `${process}.${Math.floor(Math.random() * replicas) + 1}`,
    message,
  });
}, 1100);

/** Build output for a deploy that finished before the page loaded. */
function cannedBuildLog(app: string, deployment: Deployment): LogLine[] {
  const lines: Array<[LogLine['source'], LogLine['stream'], string, string]> = [];
  if (deployment.steps.build) {
    lines.push(
      ['build', 'stdout', 'build', `==> Building ${app} with ${deployment.builder}`],
      ['build', 'stdout', 'build', '#4 [build 2/6] COPY package.json bun.lock ./'],
      ['build', 'stdout', 'build', '#5 [build 3/6] RUN bun install --frozen-lockfile'],
      ['build', 'stdout', 'build', '#5 0.912 312 packages installed [1.84s]'],
      ['build', 'stdout', 'build', '#6 [build 4/6] RUN bun run build']
    );
    if (deployment.failed_step === 'build') {
      lines.push(
        [
          'build',
          'stderr',
          'build',
          "error TS2339: Property 'variant' does not exist on type 'Product'.",
        ],
        ['build', 'stderr', 'build', '==> Build failed: exit status 2']
      );
      return toLines(app, deployment, lines);
    }
    lines.push(['build', 'stdout', 'build', `==> Image ${deployment.image_tag}`]);
  }
  if (deployment.steps.release) {
    lines.push(['build', 'stdout', 'release', '==> Running release: bun run db:migrate']);
    if (deployment.failed_step === 'release') {
      lines.push(
        ['build', 'stdout', 'release', 'Applying 20261015_add_invoice_currency.sql'],
        [
          'build',
          'stderr',
          'release',
          "ERROR 1054 (42S22): Unknown column 'currency_code' in 'invoices'",
        ],
        ['build', 'stderr', 'release', '==> Release failed: exit status 1. Nothing was switched.']
      );
      return toLines(app, deployment, lines);
    }
    lines.push(['build', 'stdout', 'release', 'No pending migrations.']);
  }
  if (deployment.steps.health_check) {
    lines.push(['build', 'stdout', 'health', '==> Health checking web.1, web.2']);
    if (deployment.failed_step === 'health_check') {
      lines.push(
        ['build', 'stderr', 'health', 'web.2: GET /healthz timed out after 30s (3 attempts)'],
        [
          'build',
          'stderr',
          'health',
          '==> Health check failed. The previous release keeps serving.',
        ]
      );
      return toLines(app, deployment, lines);
    }
    lines.push(
      ['build', 'stdout', 'health', 'web.1 ready in 2.1s'],
      ['build', 'stdout', 'health', 'web.2 ready in 2.4s']
    );
  }
  if (deployment.steps.live)
    lines.push(['build', 'stdout', 'live', '==> Switched traffic to the new release']);
  return toLines(app, deployment, lines);
}

function toLines(
  app: string,
  deployment: Deployment,
  lines: Array<[LogLine['source'], LogLine['stream'], string, string]>
): LogLine[] {
  const start = new Date(deployment.created_at).getTime();
  return lines.map(([source, stream, process, message], index) => ({
    id: `${deployment.id}-${index}`,
    app,
    deployment_id: deployment.id,
    source,
    stream,
    process,
    message,
    created_at: new Date(start + index * 4000).toISOString(),
  }));
}

// ── Deploy simulation ─────────────────────────────────────────────────────────

const wait = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms));

async function simulateDeploy(
  appName: string,
  deployment: Deployment,
  fromImage: boolean
): Promise<void> {
  const entry = findApp(appName);
  if (!entry) return;
  const step = (status: DeployStatus, name?: DeployStep) => {
    deployment.status = status;
    if (name) {
      const previous = STEP_ORDER[STEP_ORDER.indexOf(name) - 1];
      const prior = previous ? deployment.steps[previous] : undefined;
      if (prior && !prior.finished_at) prior.finished_at = new Date().toISOString();
      deployment.steps[name] = { started_at: new Date().toISOString(), finished_at: null };
    }
    emit({ type: 'deploy.status', app: appName, deployment: structuredClone(deployment) });
  };
  const log = (process: string, message: string, stream: LogLine['stream'] = 'stdout') =>
    pushLog({
      app: appName,
      deployment_id: deployment.id,
      source: 'build',
      stream,
      process,
      message,
    });

  await wait(500);
  if (!fromImage) {
    step('building', 'build');
    const buildLines = [
      `==> Building ${appName} with ${deployment.builder}`,
      '#1 [internal] load build definition from Dockerfile',
      '#2 [internal] load metadata for docker.io/oven/bun:1.3',
      '#3 [build 1/6] FROM docker.io/oven/bun:1.3@sha256:7c1f…',
      '#3 CACHED',
      '#4 [build 2/6] COPY package.json bun.lock ./',
      '#5 [build 3/6] RUN bun install --frozen-lockfile',
      '#5 0.912 312 packages installed [1.84s]',
      '#6 [build 4/6] COPY . .',
      '#7 [build 5/6] RUN bun run build',
      '#7 3.120 Compiled successfully in 2.9s',
      '#8 [runtime 6/6] COPY --from=build /app/.output ./',
      '#9 exporting to image',
      `==> Image ${deployment.image_tag}`,
    ];
    for (const line of buildLines) {
      log('build', line);
      await wait(320 + Math.random() * 380);
    }
    step('built');
    await wait(400);
  }

  step('deploying', 'release');
  log('release', '==> Running release: bun run db:migrate');
  await wait(900);
  log('release', 'Applying 20261015_add_order_notes.sql');
  await wait(1100);
  log('release', 'Migrations complete (1 applied)');
  await wait(500);

  const web = entry.processes.find((p) => p.process_type === 'web');
  const replicas = web?.scale ?? 1;
  step('health_checking', 'health_check');
  log(
    'health',
    `==> Starting ${replicas} web ${replicas === 1 ? 'replica' : 'replicas'} on the deku network`
  );
  for (let index = 1; index <= replicas; index += 1) {
    await wait(1100 + Math.random() * 600);
    log('health', `web.${index} ready in ${(1.6 + Math.random()).toFixed(1)}s`);
  }
  await wait(400);

  step('live', 'live');
  const finished = new Date().toISOString();
  if (deployment.steps.live) deployment.steps.live.finished_at = finished;
  deployment.finished_at = finished;
  entry.app.status = 'deployed';
  log('live', '==> Switched traffic to the new release');
  emit({ type: 'deploy.status', app: appName, deployment: structuredClone(deployment) });
  addEvent(
    appName,
    'deploy.live',
    fromImage ? `Rolled back to ${deployment.image_tag}` : 'Deployed from the dashboard'
  );
  emit({ type: 'app.changed', app: appName });
}

/** Checking, then issuing, then active: what the certificate watcher reports for a reachable domain. */
function simulateIssuance(appName: string, hostname: string): void {
  const step = (state: Domain['state'], delay: number, after?: () => void) =>
    window.setTimeout(() => {
      const domain = findApp(appName)?.domains.find((d) => d.hostname === hostname);
      if (domain?.mode !== 'auto') return;
      domain.state = state;
      after?.();
      emit({ type: 'app.changed', app: appName });
    }, delay);
  step('issuing', 2500);
  step('active', 6000, () => {
    const domain = findApp(appName)?.domains.find((d) => d.hostname === hostname);
    if (!domain) return;
    domain.tls = 'active';
    domain.expires_at = new Date(Date.now() + 90 * 86_400_000).toISOString();
    addEvent(appName, 'certificate.issued', `${hostname} is served over HTTPS`);
  });
}

function addEvent(app: string | null, kind: string, message: string): void {
  const event: EventRecord = {
    id: shortId(),
    app,
    kind,
    message,
    created_at: new Date().toISOString(),
  };
  store.events.unshift(event);
  emit({ type: 'event', event });
}

// ── Client ────────────────────────────────────────────────────────────────────

export const client = {
  subscribe(listener: (event: HostEvent) => void): () => void {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },

  listApps(): Promise<AppSummary[]> {
    return respond(
      store.apps.map((entry) => {
        const deployments = deploymentsOf(entry.app.name);
        return {
          app: entry.app,
          domain: entry.domains[0]?.hostname ?? null,
          processes: entry.processes,
          current: currentOf(deployments),
          active: activeOf(deployments),
          recent: deployments.slice(0, 12).map((deployment) => deployment.status),
        };
      })
    );
  },

  createApp(name: string): Promise<AppDetail> {
    if (!/^[a-z][a-z0-9-]{0,39}$/.test(name)) {
      return fail('Use lowercase letters, digits and dashes, starting with a letter');
    }
    if (findApp(name)) return fail(`${name} already exists`);
    const entry: AppDetail = {
      app: {
        id: `app_${name}`,
        name,
        created_at: new Date().toISOString(),
        locked: false,
        status: 'created',
        tls_enabled: false,
      },
      domains: [],
      processes: [],
      linked_services: [],
      maintenance: false,
      ...extras({
        ports: [],
        certificate: { path: `/etc/angie/ssl/deku_${name}.crt`, present: false, expires_at: null },
      }),
    };
    store.apps.push(entry);
    store.deployments[name] = [];
    store.config[name] = [];
    addEvent(name, 'app.created', 'App created from the dashboard');
    emit({ type: 'app.changed', app: name });
    return respond(entry);
  },

  getApp(name: string): Promise<AppDetail> {
    const entry = findApp(name);
    return entry ? respond(entry) : fail(`No app named ${name}`);
  },

  listDeployments(name: string): Promise<Deployment[]> {
    return respond(deploymentsOf(name));
  },

  getDeployment(name: string, id: string): Promise<Deployment> {
    const found = deploymentsOf(name).find((deployment) => deployment.id === id);
    return found ? respond(found) : fail(`No deployment ${id}`);
  },

  deploymentLogs(name: string, id: string): Promise<LogLine[]> {
    const live = (logs.get(name) ?? []).filter(
      (line) => line.deployment_id === id && line.source === 'build'
    );
    if (live.length > 0) return respond(live, 80);
    const deployment = deploymentsOf(name).find((entry) => entry.id === id);
    return respond(deployment ? cannedBuildLog(name, deployment) : [], 80);
  },

  recentLogs(name: string): Promise<LogLine[]> {
    return respond(
      (logs.get(name) ?? []).filter((line) => line.source === 'runtime').slice(-400),
      90
    );
  },

  /** Redeploys the app's last source, or deploys `image` when one is given. */
  startDeploy(name: string, image?: string): Promise<Deployment> {
    const entry = findApp(name);
    if (!entry) return fail(`No app named ${name}`);
    const deployments = deploymentsOf(name);
    if (activeOf(deployments)) return fail(`${name} is already deploying`);
    if (!image && deployments.length === 0) {
      return fail(
        `${name} has no source yet. Deploy an image, or run deku deploy run from a source directory.`
      );
    }
    const id = shortId();
    const builder = image ? 'image' : (deployments[0]?.builder ?? 'railpack');
    const previousImage = deployments.find(
      (deployment) => deployment.builder === 'image'
    )?.image_tag;
    const deployment: Deployment = {
      id,
      app_id: entry.app.id,
      status: 'pending',
      builder,
      image_tag:
        builder === 'image'
          ? (image ?? previousImage ?? 'nginx:1.29-alpine')
          : `deku/${name}:${id}`,
      created_at: new Date().toISOString(),
      finished_at: null,
      trigger: 'dashboard',
      steps: {},
    };
    if (entry.processes.length === 0) {
      entry.processes.push({
        process_type: 'web',
        scale: 1,
        healthy: 1,
        memory_limit: null,
        cpu_limit: null,
      });
    }
    deployments.unshift(deployment);
    emit({ type: 'deploy.status', app: name, deployment: structuredClone(deployment) });
    void simulateDeploy(name, deployment, builder === 'image');
    return respond(deployment, 60);
  },

  /** Deploys an uploaded source archive; the builder is detected from its contents. */
  uploadArchive(name: string, fileName: string): Promise<Deployment> {
    const entry = findApp(name);
    if (!entry) return fail(`No app named ${name}`);
    if (!/\.(tar\.gz|tgz|zip)$/.test(fileName))
      return fail('Upload a .tar.gz, .tgz or .zip archive');
    const deployments = deploymentsOf(name);
    if (activeOf(deployments)) return fail(`${name} is already deploying`);
    const id = shortId();
    const deployment: Deployment = {
      id,
      app_id: entry.app.id,
      status: 'pending',
      builder: 'railpack',
      image_tag: `deku/${name}:${id}`,
      created_at: new Date().toISOString(),
      finished_at: null,
      trigger: 'dashboard',
      steps: {},
    };
    if (entry.processes.length === 0) {
      entry.processes.push({
        process_type: 'web',
        scale: 1,
        healthy: 1,
        memory_limit: null,
        cpu_limit: null,
      });
    }
    deployments.unshift(deployment);
    emit({ type: 'deploy.status', app: name, deployment: structuredClone(deployment) });
    void simulateDeploy(name, deployment, false);
    return respond(deployment, 300);
  },

  rollback(name: string): Promise<Deployment> {
    const entry = findApp(name);
    const deployments = deploymentsOf(name);
    const live = deployments.filter((deployment) => deployment.status === 'live');
    if (!entry || live.length < 2) return fail('There is no earlier release to roll back to');
    const [current, previous] = live;
    const deployment: Deployment = {
      id: shortId(),
      app_id: entry.app.id,
      status: 'pending',
      builder: 'image',
      image_tag: previous.image_tag,
      created_at: new Date().toISOString(),
      finished_at: null,
      trigger: 'dashboard',
      steps: {},
    };
    deployments.unshift(deployment);
    emit({ type: 'deploy.status', app: name, deployment: structuredClone(deployment) });
    void simulateDeploy(name, deployment, true).then(() => {
      current.status = 'rolled_back';
      emit({ type: 'app.changed', app: name });
    });
    return respond(deployment, 60);
  },

  setScale(name: string, process: string, scale: number): Promise<AppDetail> {
    const entry = findApp(name);
    const record = entry?.processes.find((p) => p.process_type === process);
    if (!entry || !record) return fail('Unknown process');
    record.scale = Math.max(0, Math.min(10, scale));
    record.healthy = Math.min(record.healthy, record.scale);
    emit({ type: 'app.changed', app: name });
    window.setTimeout(() => {
      record.healthy = record.scale;
      emit({ type: 'app.changed', app: name });
    }, 1800);
    return respond(entry);
  },

  setMaintenance(name: string, on: boolean): Promise<AppDetail> {
    const entry = findApp(name);
    if (!entry) return fail(`No app named ${name}`);
    entry.maintenance = on;
    addEvent(
      name,
      on ? 'maintenance.on' : 'maintenance.off',
      on ? 'Maintenance page enabled' : 'Maintenance page disabled'
    );
    emit({ type: 'app.changed', app: name });
    return respond(entry);
  },

  listConfig(name: string): Promise<ConfigVar[]> {
    return respond(store.config[name] ?? []);
  },

  setConfig(name: string, key: string, value: string): Promise<ConfigVar[]> {
    store.config[name] ??= [];
    const vars = store.config[name];
    const existing = vars.find((entry) => entry.key === key);
    if (existing?.service) return fail(`${key} is managed by ${existing.service}`);
    if (existing) existing.value = value;
    else vars.push({ key, value });
    emit({ type: 'app.changed', app: name });
    return respond(vars);
  },

  unsetConfig(name: string, key: string): Promise<ConfigVar[]> {
    const vars = store.config[name] ?? [];
    store.config[name] = vars.filter((entry) => entry.key !== key || entry.service);
    emit({ type: 'app.changed', app: name });
    return respond(store.config[name]);
  },

  // ── App settings ────────────────────────────────────────────────────────────

  addDomain(name: string, hostname: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      const host = hostname.trim().toLowerCase();
      if (!/^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$/.test(host)) {
        throw new Error('Enter a hostname such as shop.example.com');
      }
      if (store.apps.some((app) => app.domains.some((domain) => domain.hostname === host))) {
        throw new Error(`${host} is already attached to an app`);
      }
      entry.domains.push({ hostname: host, tls: 'none', mode: 'auto', state: 'checking' });
      addEvent(name, 'domain.added', `${host} added; checking that it reaches this server`);
      simulateIssuance(name, host);
    });
  },

  /** Choose how a domain gets its certificate. */
  setDomainMode(name: string, hostname: string, mode: Domain['mode']): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      const domain = entry.domains.find((d) => d.hostname === hostname);
      if (!domain) throw new Error(`${hostname} is not attached to ${name}`);
      if (mode === 'manual' && !entry.certificate.present) {
        throw new Error(
          `Put the certificate and key at ${entry.certificate.path.replace(/\.crt$/, '.{crt,key}')} first`
        );
      }
      domain.mode = mode;
      domain.details = undefined;
      if (mode === 'manual') {
        domain.state = 'manual';
        domain.tls = 'active';
      } else if (mode === 'off') {
        domain.state = 'off';
        domain.tls = 'none';
      } else {
        domain.state = 'checking';
        domain.tls = 'none';
        simulateIssuance(name, hostname);
      }
    });
  },

  /** Check a waiting or paused domain again and request its certificate. */
  retryDomain(name: string, hostname: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      const domain = entry.domains.find((d) => d.hostname === hostname);
      if (domain?.mode !== 'auto') throw new Error(`${hostname} is not in auto mode`);
      domain.state = 'checking';
      domain.details = undefined;
      simulateIssuance(name, hostname);
    });
  },

  removeDomain(name: string, hostname: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      entry.domains = entry.domains.filter((domain) => domain.hostname !== hostname);
    });
  },

  addPort(
    name: string,
    scheme: 'http' | 'https',
    hostPort: number,
    containerPort: number
  ): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      if (entry.ports.some((port) => port.scheme === scheme && port.host_port === hostPort)) {
        throw new Error(`${scheme}:${hostPort} is already mapped`);
      }
      entry.ports.push({ scheme, host_port: hostPort, container_port: containerPort });
    });
  },

  removePort(name: string, scheme: string, hostPort: number): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      entry.ports = entry.ports.filter(
        (port) => !(port.scheme === scheme && port.host_port === hostPort)
      );
    });
  },

  addRedirect(name: string, from: string, to: string, code: Redirect['code']): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      if (!from.startsWith('/')) throw new Error('Start the path with /');
      if (!to.trim()) throw new Error('Enter where the path should go');
      entry.redirects.push({ id: shortId(), from, to: to.trim(), code });
    });
  },

  removeRedirect(name: string, id: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      entry.redirects = entry.redirects.filter((redirect) => redirect.id !== id);
    });
  },

  setAuth(name: string, auth: AuthConfig): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      if (auth.mode === 'basic' && !auth.username.trim()) throw new Error('Enter a username');
      if (auth.mode === 'forward' && !/^https?:\/\//.test(auth.url)) {
        throw new Error('Enter the auth service URL, starting with http:// or https://');
      }
      entry.auth = auth;
    });
  },

  setLimits(
    name: string,
    process: string,
    memory: string | null,
    cpu: number | null
  ): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      const record = entry.processes.find((p) => p.process_type === process);
      if (!record) throw new Error('Unknown process');
      if (memory && !/^\d+[mg]$/i.test(memory)) throw new Error('Use a size such as 512m or 1g');
      if (cpu !== null && (Number.isNaN(cpu) || cpu <= 0))
        throw new Error('Use a CPU share such as 0.5');
      record.memory_limit = memory ? memory.toLowerCase() : null;
      record.cpu_limit = cpu;
    });
  },

  addEnvironment(name: string, slug: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      if (!/^[a-z][a-z0-9-]{0,30}$/.test(slug))
        throw new Error('Use lowercase letters, digits and dashes');
      if (slug === 'production' || entry.environments.some((env) => env.slug === slug)) {
        throw new Error(`${slug} already exists`);
      }
      if (entry.environments.length === 0) {
        entry.environments.push({
          slug: 'production',
          hostname:
            entry.domains[0]?.hostname ?? `${name}.${store.acme.global_domain ?? 'localhost'}`,
          live_release: currentOf(deploymentsOf(name))?.id.slice(0, 7) ?? null,
        });
      }
      entry.environments.push({
        slug,
        hostname: `${name}-${slug}.${store.acme.global_domain ?? 'localhost'}`,
        live_release: null,
      });
    });
  },

  removeEnvironment(name: string, slug: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      if (slug === 'production') throw new Error('Production cannot be removed');
      entry.environments = entry.environments.filter((env) => env.slug !== slug);
      store.config[name] = (store.config[name] ?? []).filter((v) => v.environment !== slug);
    });
  },

  createToken(name: string, tokenName: string): Promise<{ detail: AppDetail; secret: string }> {
    const entry = findApp(name);
    if (!entry) return fail(`No app named ${name}`);
    if (!tokenName.trim()) return fail('Name the token after where it will be used');
    const secret = `dkt_${crypto.randomUUID().replaceAll('-', '')}`;
    entry.tokens.push({
      id: shortId(),
      name: tokenName.trim(),
      prefix: secret.slice(0, 8),
      created_at: new Date().toISOString(),
      last_used_at: null,
    });
    emit({ type: 'app.changed', app: name });
    return respond({ detail: entry, secret });
  },

  revokeToken(name: string, id: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      entry.tokens = entry.tokens.filter((token) => token.id !== id);
    });
  },

  attachNetwork(name: string, network: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      const target = store.networks.find((n) => n.name === network);
      if (!target) throw new Error(`No network named ${network}`);
      if (!entry.networks.includes(network)) entry.networks.push(network);
      if (!target.apps.includes(name)) target.apps.push(name);
      emit({ type: 'settings.changed' });
    });
  },

  detachNetwork(name: string, network: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      entry.networks = entry.networks.filter((n) => n !== network);
      const target = store.networks.find((n) => n.name === network);
      if (target) target.apps = target.apps.filter((app) => app !== name);
      emit({ type: 'settings.changed' });
    });
  },

  addMount(name: string, hostPath: string, containerPath: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      if (!hostPath.startsWith('/')) throw new Error('Use an absolute path on the server');
      if (!containerPath.startsWith('/'))
        throw new Error('Use an absolute path inside the container');
      entry.mounts.push({ id: shortId(), host_path: hostPath, container_path: containerPath });
    });
  },

  removeMount(name: string, id: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      entry.mounts = entry.mounts.filter((mount) => mount.id !== id);
    });
  },

  addCron(name: string, schedule: string, command: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      if (schedule.trim().split(/\s+/).length !== 5)
        throw new Error('Use five cron fields, such as 0 3 * * *');
      if (!command.trim()) throw new Error('Enter the command to run');
      entry.cron.push({ id: shortId(), schedule: schedule.trim(), command: command.trim() });
    });
  },

  removeCron(name: string, id: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      entry.cron = entry.cron.filter((job) => job.id !== id);
    });
  },

  linkObjectStore(name: string, prefix: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      if (!store.objectStore) throw new Error('Connect an object store in Settings first');
      entry.object_store = { bucket: store.objectStore.bucket, prefix };
      const vars = (store.config[name] ?? []).filter((v) => v.service !== 'object store');
      vars.push(
        { key: 'S3_BUCKET', value: store.objectStore.bucket, service: 'object store' },
        { key: 'S3_PREFIX', value: prefix, service: 'object store' },
        { key: 'S3_ACCESS_KEY_ID', value: store.objectStore.access_key_id, service: 'object store' }
      );
      store.config[name] = vars;
    });
  },

  unlinkObjectStore(name: string): Promise<AppDetail> {
    return changeApp(name, (entry) => {
      entry.object_store = null;
      store.config[name] = (store.config[name] ?? []).filter((v) => v.service !== 'object store');
    });
  },

  setEnvConfig(
    name: string,
    environment: string | undefined,
    key: string,
    value: string
  ): Promise<ConfigVar[]> {
    store.config[name] ??= [];
    const vars = store.config[name];
    const existing = vars.find((v) => v.key === key && v.environment === environment && !v.service);
    if (vars.some((v) => v.key === key && v.service))
      return fail(`${key} is managed by a linked service`);
    if (existing) existing.value = value;
    else vars.push({ key, value, environment });
    emit({ type: 'app.changed', app: name });
    return respond(vars);
  },

  unsetEnvConfig(name: string, environment: string | undefined, key: string): Promise<ConfigVar[]> {
    store.config[name] = (store.config[name] ?? []).filter(
      (v) => v.service || !(v.key === key && v.environment === environment)
    );
    emit({ type: 'app.changed', app: name });
    return respond(store.config[name]);
  },

  runHealthCheck(name: string): Promise<HealthCheckResult[]> {
    const entry = findApp(name);
    const web = entry?.processes.find((p) => p.process_type === 'web');
    if (!entry || !web) return fail('There is no web process to check');
    return respond(
      Array.from({ length: web.scale }, (_, index) => ({
        container: `web.${index + 1}`,
        status: 'pass' as const,
        latency_ms: 4 + Math.round(Math.random() * 30),
        detail: 'GET /healthz returned 200',
      })),
      700
    );
  },

  runCommand(
    name: string,
    command: string,
    mode: 'fresh' | 'live'
  ): Promise<{ exit_code: number; output: string[] }> {
    if (!command.trim()) return fail('Enter a command');
    const words = command.trim().split(/\s+/);
    const known: Record<string, string[]> = {
      ls: ['Dockerfile', 'package.json', 'bun.lock', 'src', 'public', 'node_modules'],
      env: [
        'NODE_ENV=production',
        'PORT=3000',
        'DATABASE_URL=postgres://shop:****@deku-postgres-shop-db:5432/shop',
      ],
      whoami: ['app'],
      pwd: ['/app'],
    };
    const output = known[words[0]] ?? [
      mode === 'fresh'
        ? `Started a fresh container from the ${name} image.`
        : `Running in ${name} web.1.`,
      `$ ${command.trim()}`,
      'Done.',
    ];
    return respond({ exit_code: 0, output }, 900);
  },

  // ── Host and global settings ───────────────────────────────────────────────

  getToken(): Promise<DashboardToken> {
    return respond(store.token);
  },

  rotateToken(): Promise<{ token: DashboardToken; secret: string }> {
    const secret = `dku_${crypto.randomUUID().replaceAll('-', '')}`;
    store.token = {
      prefix: secret.slice(0, 8),
      created_at: new Date().toISOString(),
      expires_at: new Date(Date.now() + 30 * 86_400_000).toISOString(),
    };
    emit({ type: 'settings.changed' });
    return respond({ token: store.token, secret });
  },

  getAcme(): Promise<AcmeSettings> {
    return respond(store.acme);
  },

  setAcme(
    next: Partial<Pick<AcmeSettings, 'enabled' | 'email' | 'global_domain'>>
  ): Promise<AcmeSettings> {
    if (next.email !== undefined && next.email && !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(next.email)) {
      return fail('Enter an email address such as ops@example.com');
    }
    store.acme = { ...store.acme, ...next };
    emit({ type: 'settings.changed' });
    return respond(store.acme);
  },

  setDnsToken(token: string): Promise<AcmeSettings> {
    if (token.trim().length < 20) return fail('That does not look like a Cloudflare API token');
    store.acme.wildcard = { provider: 'cloudflare', token_set: true, status: 'pending' };
    emit({ type: 'settings.changed' });
    window.setTimeout(() => {
      store.acme.wildcard.status = 'issued';
      emit({ type: 'settings.changed' });
    }, 2500);
    return respond(store.acme, 600);
  },

  getObjectStore(): Promise<ObjectStore | null> {
    return respond(store.objectStore);
  },

  setObjectStore(next: ObjectStore): Promise<ObjectStore> {
    if (!/^https:\/\//.test(next.endpoint))
      return fail('Use the https:// endpoint from your provider');
    if (!next.bucket.trim()) return fail('Enter the bucket name');
    store.objectStore = next;
    emit({ type: 'settings.changed' });
    return respond(next);
  },

  testObjectStore(): Promise<{ ok: boolean; detail: string }> {
    if (!store.objectStore) return fail('No object store is configured');
    return respond(
      { ok: true, detail: `Wrote and read a test object in ${store.objectStore.bucket}.` },
      900
    );
  },

  removeObjectStore(): Promise<null> {
    store.objectStore = null;
    emit({ type: 'settings.changed' });
    return respond(null);
  },

  listSshKeys(): Promise<SshKey[]> {
    return respond(store.sshKeys);
  },

  addSshKey(name: string, publicKey: string): Promise<SshKey[]> {
    const parts = publicKey.trim().split(/\s+/);
    if (!/^(ssh-ed25519|ssh-rsa|ecdsa-sha2-nistp\d+)$/.test(parts[0] ?? '') || !parts[1]) {
      return fail('Paste a public key that starts with ssh-ed25519, ssh-rsa or ecdsa-sha2');
    }
    store.sshKeys.push({
      fingerprint: `SHA256:${parts[1].slice(-32)}`,
      name: name.trim() || parts[2] || 'unnamed key',
      added_at: new Date().toISOString(),
    });
    emit({ type: 'settings.changed' });
    return respond(store.sshKeys);
  },

  removeSshKey(fingerprint: string): Promise<SshKey[]> {
    store.sshKeys = store.sshKeys.filter((key) => key.fingerprint !== fingerprint);
    emit({ type: 'settings.changed' });
    return respond(store.sshKeys);
  },

  listNetworks(): Promise<DockerNetwork[]> {
    return respond(store.networks);
  },

  createNetwork(name: string): Promise<DockerNetwork[]> {
    if (!/^[a-z][a-z0-9_.-]{1,40}$/.test(name))
      return fail('Use lowercase letters, digits, dots, dashes or underscores');
    if (store.networks.some((network) => network.name === name))
      return fail(`${name} already exists`);
    store.networks.push({ name, driver: 'bridge', managed: false, apps: [] });
    emit({ type: 'settings.changed' });
    return respond(store.networks);
  },

  removeNetwork(name: string): Promise<DockerNetwork[]> {
    const network = store.networks.find((n) => n.name === name);
    if (!network) return fail(`No network named ${name}`);
    if (network.managed) return fail('The deku network is managed by Deku and cannot be removed');
    for (const entry of store.apps) entry.networks = entry.networks.filter((n) => n !== name);
    store.networks = store.networks.filter((n) => n.name !== name);
    emit({ type: 'settings.changed' });
    for (const app of network.apps) emit({ type: 'app.changed', app });
    return respond(store.networks);
  },

  listRoutes(): Promise<Route[]> {
    const routes: Route[] = [];
    for (const entry of store.apps) {
      const web = entry.processes.find((p) => p.process_type === 'web');
      const upstreams = Array.from(
        { length: web?.scale ?? 0 },
        (_, index) => `127.0.0.1:${49152 + store.apps.indexOf(entry) * 16 + index}`
      );
      for (const domain of entry.domains) {
        routes.push({
          hostname: domain.hostname,
          app: entry.app.name,
          environment: 'production',
          upstreams,
          tls: domain.tls,
        });
      }
      for (const env of entry.environments.filter((e) => e.slug !== 'production')) {
        routes.push({
          hostname: env.hostname,
          app: entry.app.name,
          environment: env.slug,
          upstreams: upstreams.slice(0, 1),
          tls: store.acme.wildcard.status === 'issued' ? 'active' : 'none',
        });
      }
    }
    return respond(routes);
  },

  getProxy(): Promise<ProxyStatus> {
    return respond(store.proxy);
  },

  reloadProxy(): Promise<ProxyStatus> {
    store.proxy = { ...store.proxy, valid: true, reloaded_at: new Date().toISOString() };
    emit({ type: 'settings.changed' });
    return respond(store.proxy, 500);
  },

  // ── Services ─────────────────────────────────────────────────────────────────

  createService(kind: ServiceKind, name: string): Promise<ManagedService> {
    if (!/^[a-z][a-z0-9-]{0,39}$/.test(name))
      return fail('Use lowercase letters, digits and dashes, starting with a letter');
    if (store.services.some((service) => service.name === name))
      return fail(`${name} already exists`);
    const versions: Record<ServiceKind, string> = {
      postgres: '17.6',
      mysql: '8.4',
      mariadb: '11.8',
      redis: '8.2',
      mongodb: '8.0',
    };
    const service: ManagedService = {
      name,
      kind,
      version: versions[kind],
      status: 'running',
      linked_apps: [],
      volume_size: '0 MB',
      last_backup_at: null,
      backup_schedule: null,
    };
    store.services.push(service);
    store.backups[name] = [];
    addEvent(null, 'service.created', `${kind} service ${name} created`);
    emit({ type: 'services.changed' });
    return respond(service, 900);
  },

  linkService(service: string, app: string): Promise<ManagedService> {
    const target = store.services.find((entry) => entry.name === service);
    const entry = findApp(app);
    if (!target || !entry) return fail('Unknown service or app');
    if (!target.linked_apps.includes(app)) target.linked_apps.push(app);
    if (!entry.linked_services.includes(service)) entry.linked_services.push(service);
    const key =
      target.kind === 'redis'
        ? 'REDIS_URL'
        : target.kind === 'mongodb'
          ? 'MONGODB_URL'
          : 'DATABASE_URL';
    const scheme =
      target.kind === 'mariadb' ? 'mysql' : target.kind === 'mongodb' ? 'mongodb' : target.kind;
    store.config[app] = (store.config[app] ?? []).filter((v) => v.key !== key);
    store.config[app].push({
      key,
      value: `${scheme}://${app}:••••@deku-${target.kind}-${service}/${app}`,
      service,
    });
    emit({ type: 'services.changed' });
    emit({ type: 'app.changed', app });
    return respond(target);
  },

  unlinkService(service: string, app: string): Promise<ManagedService> {
    const target = store.services.find((entry) => entry.name === service);
    const entry = findApp(app);
    if (!target || !entry) return fail('Unknown service or app');
    target.linked_apps = target.linked_apps.filter((name) => name !== app);
    entry.linked_services = entry.linked_services.filter((name) => name !== service);
    store.config[app] = (store.config[app] ?? []).filter((v) => v.service !== service);
    emit({ type: 'services.changed' });
    emit({ type: 'app.changed', app });
    return respond(target);
  },

  serviceConnection(name: string): Promise<{ url: string; host: string; port: number }> {
    const service = store.services.find((entry) => entry.name === name);
    if (!service) return fail(`No service named ${name}`);
    const ports: Record<ServiceKind, number> = {
      postgres: 5432,
      mysql: 3306,
      mariadb: 3306,
      redis: 6379,
      mongodb: 27017,
    };
    const host = `deku-${service.kind}-${name}`;
    const scheme = service.kind === 'mariadb' ? 'mysql' : service.kind;
    const user = service.kind === 'redis' ? '' : `${name.replaceAll('-', '_')}`;
    return respond({
      url: `${scheme}://${user}:9f2c41d07be3@${host}:${ports[service.kind]}/${user}`,
      host,
      port: ports[service.kind],
    });
  },

  serviceLogs(name: string): Promise<string[]> {
    const service = store.services.find((entry) => entry.name === name);
    if (!service) return fail(`No service named ${name}`);
    const lines: Record<ServiceKind, string[]> = {
      postgres: [
        'LOG:  checkpoint starting: time',
        'LOG:  checkpoint complete: wrote 412 buffers (2.5%)',
        'LOG:  database system is ready to accept connections',
      ],
      mysql: [
        '[Server] /usr/sbin/mysqld: ready for connections. Version: 8.4.6',
        '[InnoDB] Buffer pool(s) load completed',
      ],
      mariadb: [
        '[Note] mariadbd: ready for connections.',
        '[Note] InnoDB: Buffer pool(s) load completed',
      ],
      redis: [
        '* Background saving started by pid 214',
        '* DB saved on disk',
        '* Ready to accept connections tcp',
      ],
      mongodb: [
        '{"msg":"Waiting for connections","attr":{"port":27017}}',
        '{"msg":"Connection accepted"}',
      ],
    };
    return respond(lines[service.kind], 100);
  },

  listBackups(name: string): Promise<ServiceBackup[]> {
    return respond(store.backups[name] ?? []);
  },

  restoreBackup(name: string, id: string): Promise<null> {
    const backup = (store.backups[name] ?? []).find((entry) => entry.id === id);
    if (!backup) return fail('That backup no longer exists');
    window.setTimeout(() => {
      addEvent(null, 'backup.restored', `${name} restored from ${id}`);
      emit({ type: 'services.changed' });
    }, 1800);
    return respond(null, 300);
  },

  setBackupSchedule(
    name: string,
    schedule: ManagedService['backup_schedule']
  ): Promise<ManagedService> {
    const service = store.services.find((entry) => entry.name === name);
    if (!service) return fail(`No service named ${name}`);
    if (schedule && !store.objectStore)
      return fail('Connect an object store in Settings before scheduling backups');
    service.backup_schedule = schedule;
    emit({ type: 'services.changed' });
    return respond(service);
  },

  listServices(): Promise<ManagedService[]> {
    return respond(store.services);
  },

  backupService(name: string): Promise<ManagedService> {
    const service = store.services.find((entry) => entry.name === name);
    if (!service) return fail(`No service named ${name}`);
    if (!store.objectStore) return fail('Connect an object store in Settings before backing up');
    window.setTimeout(() => {
      service.last_backup_at = new Date().toISOString();
      store.backups[name] ??= [];
      store.backups[name].unshift({
        id: `b-${shortId().slice(-4)}`,
        created_at: service.last_backup_at,
        size: service.volume_size,
      });
      addEvent(null, 'backup.completed', `${name} backed up`);
      emit({ type: 'services.changed' });
    }, 2200);
    return respond(service);
  },

  listChecks(): Promise<DoctorCheck[]> {
    return respond(store.checks);
  },

  getHost(): Promise<HostStatus> {
    return respond(store.host);
  },

  listEvents(): Promise<EventRecord[]> {
    return respond(store.events.slice(0, 40));
  },
};
