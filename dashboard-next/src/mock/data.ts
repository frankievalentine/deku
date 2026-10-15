import type {
  AcmeSettings,
  App,
  AppDetail,
  ConfigVar,
  DashboardToken,
  Deployment,
  DeployStep,
  DockerNetwork,
  DoctorCheck,
  EventRecord,
  HostStatus,
  ManagedService,
  ObjectStore,
  ProxyStatus,
  ServiceBackup,
  SshKey,
} from '../lib/types';

export const STEP_ORDER: DeployStep[] = ['build', 'release', 'health_check', 'live'];

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

export function ago(ms: number): string {
  return new Date(Date.now() - ms).toISOString();
}

let sequence = 0;
export function shortId(): string {
  sequence += 1;
  return (Date.now().toString(16).slice(-5) + sequence.toString(16).padStart(3, '0')).slice(-8);
}

function app(name: string, status: App['status'], createdDaysAgo: number, tls = true): App {
  return {
    id: `app_${name}`,
    name,
    created_at: ago(createdDaysAgo * DAY),
    locked: false,
    status,
    tls_enabled: tls,
  };
}

type AppExtras = Pick<
  AppDetail,
  | 'ports'
  | 'redirects'
  | 'auth'
  | 'environments'
  | 'tokens'
  | 'networks'
  | 'mounts'
  | 'cron'
  | 'object_store'
  | 'certificate'
>;

export function extras(partial: Partial<AppExtras> = {}): AppExtras {
  return {
    ports: [{ scheme: 'http', host_port: 80, container_port: 3000 }],
    redirects: [],
    auth: { mode: 'none' },
    environments: [],
    tokens: [],
    networks: [],
    mounts: [],
    cron: [],
    object_store: null,
    certificate: { path: '', present: false, expires_at: null },
    ...partial,
  };
}

/** Where Deku reads an app's certificate, and whether the operator has put one there. */
function cert(app: string, daysLeft: number | null): AppDetail['certificate'] {
  return {
    path: `/etc/angie/ssl/deku_${app}.crt`,
    present: daysLeft !== null,
    expires_at: daysLeft === null ? null : ago(-daysLeft * DAY),
  };
}

export interface MockStore {
  token: DashboardToken;
  acme: AcmeSettings;
  objectStore: ObjectStore | null;
  sshKeys: SshKey[];
  networks: DockerNetwork[];
  proxy: ProxyStatus;
  backups: Record<string, ServiceBackup[]>;
  apps: AppDetail[];
  deployments: Record<string, Deployment[]>;
  config: Record<string, ConfigVar[]>;
  services: ManagedService[];
  checks: DoctorCheck[];
  host: HostStatus;
  events: EventRecord[];
}

/** A stable 32-bit FNV-1a hash, so sample IDs and timings look varied but never change. */
function hash(text: string): number {
  let value = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    value ^= text.charCodeAt(index);
    value = Math.imul(value, 0x01000193) >>> 0;
  }
  return value;
}

/** A deploy history, newest first, with a few failures and rollbacks among the successes. */
function history(
  appName: string,
  builder: Deployment['builder'],
  entries: Array<[number, Deployment['status'], Deployment['trigger'], Deployment['failed_step']?]>
): Deployment[] {
  return entries.map(([hoursAgo, status, trigger, failedStep], index) => {
    const created = hoursAgo * HOUR;
    const noise = hash(`${appName}:${index}`);
    const duration = (status === 'failed' ? 25 + (noise % 40) : 48 + (noise % 110)) * 1000;
    const id = (noise.toString(16) + hash(`${noise}`).toString(16)).slice(0, 8);
    const reached: DeployStep[] = failedStep
      ? STEP_ORDER.slice(0, STEP_ORDER.indexOf(failedStep) + 1)
      : builder === 'image'
        ? STEP_ORDER.slice(1)
        : STEP_ORDER;
    const steps: Deployment['steps'] = {};
    const share = duration / reached.length;
    reached.forEach((step, stepIndex) => {
      const start = created - share * stepIndex;
      steps[step] = { started_at: ago(start), finished_at: ago(start - share) };
    });
    return {
      id,
      app_id: `app_${appName}`,
      status,
      builder,
      image_tag: builder === 'image' ? 'nginx:1.29-alpine' : `deku/${appName}:${id}`,
      created_at: ago(created),
      finished_at: ago(created - duration),
      failed_step: failedStep,
      trigger,
      steps,
    };
  });
}

export function seed(): MockStore {
  return {
    token: { prefix: 'dku_7f3a', created_at: ago(12 * DAY), expires_at: ago(-18 * DAY) },
    acme: {
      enabled: true,
      email: 'ops@example.com',
      global_domain: 'example.com',
      wildcard: { provider: 'cloudflare', token_set: true, status: 'issued' },
    },
    objectStore: {
      provider: 'Cloudflare R2',
      endpoint: 'https://3f1c0a.r2.cloudflarestorage.com',
      bucket: 'deku-backups',
      region: 'auto',
      access_key_id: 'a81f0c2e9b',
    },
    sshKeys: [
      {
        fingerprint: 'SHA256:qX0v4cT1mP9rB2kLw8Zy3nH6dE5sF7gJ',
        name: 'laptop',
        added_at: ago(40 * DAY),
      },
      {
        fingerprint: 'SHA256:Lm2N8pQ4rS6tU1vW3xY5zA7bC9dE0fG2',
        name: 'ci@github',
        added_at: ago(30 * DAY),
      },
    ],
    networks: [
      { name: 'deku', driver: 'bridge', managed: true, apps: ['shop', 'api', 'billing', 'docs'] },
      { name: 'payments-net', driver: 'bridge', managed: false, apps: ['api', 'billing'] },
    ],
    proxy: { valid: true, reloaded_at: ago(3 * HOUR), config_path: '/etc/angie/http.d', files: 7 },
    backups: {
      'shop-db': [
        { id: 'b-0c41', created_at: ago(7 * HOUR), size: '312 MB' },
        { id: 'b-9a12', created_at: ago(31 * HOUR), size: '309 MB' },
        { id: 'b-77fe', created_at: ago(55 * HOUR), size: '305 MB' },
      ],
      'shop-cache': [],
      'billing-db': [{ id: 'b-1d08', created_at: ago(3 * DAY), size: '88 MB' }],
    },
    apps: [
      {
        app: app('shop', 'deployed', 41),
        domains: [
          {
            hostname: 'shop.example.com',
            tls: 'active',
            mode: 'auto',
            state: 'active',
            expires_at: ago(-75 * DAY),
          },
          {
            hostname: 'www.shop.example.com',
            tls: 'active',
            mode: 'auto',
            state: 'active',
            expires_at: ago(-75 * DAY),
          },
        ],
        processes: [
          { process_type: 'web', scale: 2, healthy: 2, memory_limit: '512m', cpu_limit: 0.5 },
          { process_type: 'worker', scale: 1, healthy: 1, memory_limit: '256m', cpu_limit: null },
        ],
        linked_services: ['shop-db', 'shop-cache'],
        maintenance: false,
        ...extras({
          ports: [
            { scheme: 'http', host_port: 80, container_port: 3000 },
            { scheme: 'https', host_port: 443, container_port: 3000 },
          ],
          redirects: [
            { id: 'r1', from: '/catalog', to: '/products', code: 301 },
            { id: 'r2', from: '/blog', to: 'https://blog.example.com', code: 302 },
          ],
          environments: [
            { slug: 'production', hostname: 'shop.example.com', live_release: 'c9a7111' },
            { slug: 'staging', hostname: 'shop-staging.example.com', live_release: '8a31f02' },
          ],
          tokens: [
            {
              id: 't1',
              name: 'github-actions',
              prefix: 'dkt_4be1',
              created_at: ago(30 * DAY),
              last_used_at: ago(3 * HOUR),
            },
          ],
          mounts: [
            {
              host_path: '/var/lib/deku/storage/shop/uploads',
              container_path: '/app/public/uploads',
              id: 'm1',
            },
          ],
          cron: [
            { id: 'c1', schedule: '*/15 * * * *', command: 'bun run jobs/sync-inventory.ts' },
            { id: 'c2', schedule: '0 3 * * *', command: 'bun run jobs/prune-carts.ts' },
          ],
          object_store: { bucket: 'deku-backups', prefix: 'apps/shop/' },
          certificate: cert('shop', 52),
        }),
      },
      {
        app: app('api', 'deployed', 63),
        domains: [{ hostname: 'api.example.com', tls: 'active', mode: 'manual', state: 'manual' }],
        processes: [
          { process_type: 'web', scale: 3, healthy: 3, memory_limit: '384m', cpu_limit: 1 },
        ],
        linked_services: ['shop-db'],
        maintenance: false,
        ...extras({
          ports: [{ scheme: 'http', host_port: 80, container_port: 8080 }],
          auth: { mode: 'forward', url: 'http://deku-auth-proxy:4180/oauth2/auth' },
          tokens: [
            {
              id: 't2',
              name: 'github-actions',
              prefix: 'dkt_91c0',
              created_at: ago(60 * DAY),
              last_used_at: ago(0.6 * HOUR),
            },
            {
              id: 't3',
              name: 'buildkite',
              prefix: 'dkt_0d7e',
              created_at: ago(90 * DAY),
              last_used_at: null,
            },
          ],
          certificate: cert('api', 71),
          networks: ['payments-net'],
        }),
      },
      {
        app: app('billing', 'deployed', 12),
        domains: [
          {
            hostname: 'billing.example.com',
            tls: 'active',
            mode: 'auto',
            state: 'active',
            expires_at: ago(-61 * DAY),
          },
        ],
        processes: [
          { process_type: 'web', scale: 1, healthy: 1, memory_limit: '256m', cpu_limit: null },
          { process_type: 'scheduler', scale: 1, healthy: 1, memory_limit: null, cpu_limit: null },
        ],
        linked_services: ['billing-db'],
        maintenance: false,
        ...extras({
          ports: [{ scheme: 'http', host_port: 80, container_port: 8000 }],
          auth: { mode: 'basic', username: 'finance' },
          networks: ['payments-net'],
          cron: [{ id: 'c3', schedule: '0 6 1 * *', command: 'php artisan invoices:run' }],
          certificate: cert('billing', 9),
        }),
      },
      {
        app: app('docs', 'deployed', 90, false),
        domains: [
          {
            hostname: 'docs.example.com',
            tls: 'none',
            mode: 'auto',
            state: 'checking',
            details:
              'docs.example.com resolves to 203.0.113.40 but nothing answered on port 80 there. Point the record at this server and allow port 80.',
          },
        ],
        processes: [
          { process_type: 'web', scale: 1, healthy: 1, memory_limit: '128m', cpu_limit: null },
        ],
        linked_services: [],
        maintenance: false,
        ...extras({
          ports: [{ scheme: 'http', host_port: 80, container_port: 80 }],
          certificate: cert('docs', null),
        }),
      },
      {
        app: app('labs', 'created', 1, false),
        domains: [],
        processes: [],
        linked_services: [],
        maintenance: false,
        ...extras({ ports: [] }),
      },
    ],
    deployments: {
      shop: history('shop', 'railpack', [
        [3, 'live', 'git'],
        [27, 'rolled_back', 'git'],
        [29, 'failed', 'git', 'health_check'],
        [52, 'live', 'cli'],
        [75, 'live', 'git'],
        [98, 'live', 'git'],
        [140, 'failed', 'cli', 'build'],
        [161, 'live', 'git'],
      ]),
      api: history('api', 'dockerfile', [
        [0.6, 'live', 'token'],
        [6, 'live', 'token'],
        [30, 'live', 'token'],
        [55, 'live', 'token'],
        [80, 'live', 'token'],
        [102, 'live', 'token'],
      ]),
      billing: history('billing', 'dockerfile', [
        [0.3, 'failed', 'cli', 'release'],
        [20, 'live', 'cli'],
        [44, 'live', 'cli'],
        [70, 'failed', 'cli', 'release'],
        [71, 'live', 'cli'],
      ]),
      docs: history('docs', 'image', [
        [200, 'live', 'dashboard'],
        [420, 'live', 'dashboard'],
      ]),
      labs: [],
    },
    config: {
      shop: [
        {
          key: 'DATABASE_URL',
          value: 'postgres://shop:9f2c…@deku-postgres-shop-db:5432/shop',
          service: 'shop-db',
        },
        {
          key: 'REDIS_URL',
          value: 'redis://:a81d…@deku-redis-shop-cache:6379',
          service: 'shop-cache',
        },
        { key: 'NODE_ENV', value: 'production' },
        { key: 'STRIPE_SECRET_KEY', value: 'sk_live_51N…q8' },
        { key: 'SESSION_SECRET', value: '3c1e9b7f0a…' },
        { key: 'PUBLIC_URL', value: 'https://shop.example.com' },
        { key: 'PUBLIC_URL', value: 'https://shop-staging.example.com', environment: 'staging' },
        { key: 'STRIPE_SECRET_KEY', value: 'sk_test_51N…x2', environment: 'staging' },
        { key: 'S3_BUCKET', value: 'deku-backups', service: 'object store' },
        { key: 'S3_PREFIX', value: 'apps/shop/', service: 'object store' },
        { key: 'S3_ACCESS_KEY_ID', value: 'a81f0c2e9b', service: 'object store' },
      ],
      api: [
        {
          key: 'DATABASE_URL',
          value: 'postgres://api:71bb…@deku-postgres-shop-db:5432/shop',
          service: 'shop-db',
        },
        { key: 'RUST_LOG', value: 'info' },
        { key: 'JWT_SECRET', value: 'e0f4…' },
      ],
      billing: [
        {
          key: 'DATABASE_URL',
          value: 'mysql://billing:c3d0…@deku-mysql-billing-db:3306/billing',
          service: 'billing-db',
        },
        { key: 'APP_ENV', value: 'production' },
      ],
      docs: [],
      labs: [],
    },
    services: [
      {
        name: 'shop-db',
        kind: 'postgres',
        version: '17.6',
        status: 'running',
        linked_apps: ['shop', 'api'],
        volume_size: '2.4 GB',
        last_backup_at: ago(7 * HOUR),
        backup_schedule: { interval_hours: 24, keep: 7 },
      },
      {
        name: 'shop-cache',
        kind: 'redis',
        version: '8.2',
        status: 'running',
        linked_apps: ['shop'],
        volume_size: '48 MB',
        last_backup_at: null,
        backup_schedule: null,
      },
      {
        name: 'billing-db',
        kind: 'mysql',
        version: '8.4',
        status: 'running',
        linked_apps: ['billing'],
        volume_size: '610 MB',
        last_backup_at: ago(3 * DAY),
        backup_schedule: null,
      },
    ],
    checks: [
      {
        id: 'disk',
        name: 'Disk space',
        status: 'warn',
        detail: '/var/lib/docker is 84% full (67 of 80 GB).',
        fix: 'Remove unused images and build cache.',
        command: 'docker system prune --filter until=168h',
      },
      {
        id: 'backups',
        name: 'Backup coverage',
        status: 'warn',
        detail: 'billing-db and shop-cache have no backup schedule.',
        fix: 'Schedule a daily backup for each service you cannot rebuild.',
        command: 'deku backup schedule billing-db --interval-hours 24 --keep 7',
      },
      {
        id: 'cert',
        name: 'Certificates',
        status: 'warn',
        detail:
          'docs.example.com does not reach this server yet, so it is served over HTTP and no certificate is requested.',
        fix: 'Point its DNS record at this server and allow port 80. The certificate follows within a minute.',
        command: 'deku domains list docs',
      },
      {
        id: 'docker',
        name: 'Docker',
        status: 'pass',
        detail: 'Engine 28.4 reachable over the socket.',
      },
      {
        id: 'proxy',
        name: 'Proxy configuration',
        status: 'pass',
        detail: 'Angie config is valid and loaded.',
      },
      { id: 'db', name: 'State database', status: 'pass', detail: 'Integrity check passed.' },
      {
        id: 'network',
        name: 'Container network',
        status: 'pass',
        detail: 'Every Deku container is on the deku network.',
      },
      {
        id: 'ports',
        name: 'Published ports',
        status: 'pass',
        detail: 'No container publishes beyond loopback.',
      },
      {
        id: 'api',
        name: 'API bind address',
        status: 'pass',
        detail: 'Listening on 127.0.0.1:2810 only.',
      },
      {
        id: 'key',
        name: 'Encryption key',
        status: 'pass',
        detail: 'Config values and backups are encrypted.',
      },
    ],
    host: {
      hostname: 'deku-prod-1',
      version: '0.1.12',
      latest_version: '0.1.13',
      uptime_seconds: 19 * 86_400 + 4 * 3600,
      disk: { used_gb: 67, total_gb: 80 },
      memory: { used_mb: 2860, total_mb: 7940 },
      cpu_percent: 18,
      load: [0.42, 0.51, 0.47],
    },
    events: [
      {
        id: 'e1',
        app: 'billing',
        kind: 'deploy.failed',
        message: 'Release command exited with status 1',
        created_at: ago(0.3 * HOUR),
      },
      {
        id: 'e2',
        app: 'api',
        kind: 'deploy.live',
        message: 'Deployed by token "github-actions"',
        created_at: ago(0.6 * HOUR),
      },
      {
        id: 'e3',
        app: 'shop',
        kind: 'deploy.live',
        message: 'Deployed from git push',
        created_at: ago(3 * HOUR),
      },
      {
        id: 'e4',
        app: null,
        kind: 'backup.completed',
        message: 'shop-db backed up (312 MB)',
        created_at: ago(7 * HOUR),
      },
      {
        id: 'e5',
        app: 'shop',
        kind: 'certificate.issued',
        message: 'www.shop.example.com is served over HTTPS',
        created_at: ago(9 * HOUR),
      },
      {
        id: 'e6',
        app: 'shop',
        kind: 'deploy.failed',
        message: 'Health check failed on web.2; kept the previous release',
        created_at: ago(29 * HOUR),
      },
    ],
  };
}
