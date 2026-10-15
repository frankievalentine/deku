// Shapes follow the daemon's HTTP API (see dashboard/src/lib/api.ts and /api/openapi.json), so the
// mock client can be replaced by a generated client without touching the screens.

export type AppStatus = 'created' | 'deployed' | 'stopped' | 'error';

export interface App {
  id: string;
  name: string;
  created_at: string;
  locked: boolean;
  status: AppStatus;
  tls_enabled: boolean;
}

export type DeployStatus =
  | 'pending'
  | 'building'
  | 'built'
  | 'deploying'
  | 'health_checking'
  | 'live'
  | 'failed'
  | 'rolled_back';

export type Builder = 'dockerfile' | 'railpack' | 'image';

export interface Deployment {
  id: string;
  app_id: string;
  status: DeployStatus;
  builder: Builder;
  image_tag: string | null;
  created_at: string;
  finished_at: string | null;
  /** Which step failed, when the deploy failed. */
  failed_step?: DeployStep;
  /** Where the deploy came from: CLI, git push, or a deploy token. */
  trigger: 'cli' | 'git' | 'token' | 'dashboard';
  /** When each step started and finished, as far as the deploy got. */
  steps: Partial<Record<DeployStep, StepTiming>>;
}

export interface StepTiming {
  started_at: string;
  finished_at: string | null;
}

export type DeployStep = 'build' | 'release' | 'health_check' | 'live';

export type TlsMode = 'auto' | 'manual' | 'off';
export type CertState = 'pending' | 'checking' | 'issuing' | 'active' | 'failed' | 'manual' | 'off';

export interface Domain {
  hostname: string;
  /** Whether the domain is served over HTTPS right now. */
  tls: 'active' | 'none';
  mode: TlsMode;
  state: CertState;
  /** Why the domain is waiting or paused, in words the operator can act on. */
  details?: string;
  expires_at?: string;
}

/** The certificate an operator installed for an app's own domains. */
export interface AppCertificate {
  path: string;
  present: boolean;
  expires_at: string | null;
}

export interface ProcessRecord {
  process_type: string;
  scale: number;
  healthy: number;
  memory_limit: string | null;
  cpu_limit: number | null;
}

export interface ConfigVar {
  key: string;
  value: string;
  /** A linked service owns this variable; it changes when the service does. */
  service?: string;
  /** Set only for one environment, overriding the app-wide value. */
  environment?: string;
}

export interface PortMapping {
  scheme: 'http' | 'https';
  host_port: number;
  container_port: number;
}

export interface Redirect {
  id: string;
  from: string;
  to: string;
  code: 301 | 302 | 307 | 308;
}

export type AuthConfig =
  | { mode: 'none' }
  | { mode: 'basic'; username: string }
  | { mode: 'forward'; url: string };

export interface Environment {
  slug: string;
  hostname: string;
  live_release: string | null;
}

export interface DeployToken {
  id: string;
  name: string;
  prefix: string;
  created_at: string;
  last_used_at: string | null;
}

export interface Mount {
  id: string;
  host_path: string;
  container_path: string;
}

export interface CronEntry {
  id: string;
  schedule: string;
  command: string;
}

export interface AppObjectStoreLink {
  bucket: string;
  prefix: string;
}

export interface HealthCheckResult {
  container: string;
  status: 'pass' | 'fail';
  latency_ms: number | null;
  detail: string;
}

export type ServiceKind = 'postgres' | 'mysql' | 'mariadb' | 'redis' | 'mongodb';

export interface ManagedService {
  name: string;
  kind: ServiceKind;
  version: string;
  status: 'running' | 'stopped' | 'error';
  linked_apps: string[];
  volume_size: string;
  last_backup_at: string | null;
  backup_schedule: { interval_hours: number; keep: number } | null;
}

export type LogStream = 'stdout' | 'stderr';

export interface LogLine {
  id: string;
  app: string;
  deployment_id: string | null;
  source: 'build' | 'runtime';
  stream: LogStream;
  process: string;
  message: string;
  created_at: string;
}

export type CheckStatus = 'pass' | 'warn' | 'fail';

export interface DoctorCheck {
  id: string;
  name: string;
  status: CheckStatus;
  detail: string;
  /** What to do about it, for anything that is not passing. */
  fix?: string;
  command?: string;
}

export interface HostStatus {
  hostname: string;
  version: string;
  latest_version: string;
  uptime_seconds: number;
  disk: { used_gb: number; total_gb: number };
  memory: { used_mb: number; total_mb: number };
  cpu_percent: number;
  load: number[];
}

export interface EventRecord {
  id: string;
  app: string | null;
  kind: string;
  message: string;
  created_at: string;
}

/** One row of the apps list: the app plus what the list shows about it. */
export interface AppSummary {
  app: App;
  domain: string | null;
  processes: ProcessRecord[];
  current: Deployment | null;
  active: Deployment | null;
  recent: DeployStatus[];
}

export interface AppDetail {
  app: App;
  domains: Domain[];
  processes: ProcessRecord[];
  linked_services: string[];
  maintenance: boolean;
  ports: PortMapping[];
  redirects: Redirect[];
  auth: AuthConfig;
  environments: Environment[];
  tokens: DeployToken[];
  networks: string[];
  mounts: Mount[];
  cron: CronEntry[];
  object_store: AppObjectStoreLink | null;
  certificate: AppCertificate;
}

export interface DashboardToken {
  prefix: string;
  created_at: string;
  expires_at: string;
}

export interface AcmeSettings {
  enabled: boolean;
  email: string | null;
  global_domain: string | null;
  wildcard: { provider: 'cloudflare'; token_set: boolean; status: 'issued' | 'pending' | 'off' };
}

export interface ObjectStore {
  provider: string;
  endpoint: string;
  bucket: string;
  region: string;
  access_key_id: string;
}

export interface SshKey {
  fingerprint: string;
  name: string;
  added_at: string;
}

export interface DockerNetwork {
  name: string;
  driver: string;
  managed: boolean;
  apps: string[];
}

export interface Route {
  hostname: string;
  app: string;
  environment: string;
  upstreams: string[];
  tls: 'active' | 'none';
}

export interface ProxyStatus {
  valid: boolean;
  reloaded_at: string;
  config_path: string;
  files: number;
}

export interface ServiceBackup {
  id: string;
  created_at: string;
  size: string;
}

/** One push from the host-wide event stream. */
export type HostEvent =
  | { type: 'deploy.status'; app: string; deployment: Deployment }
  | { type: 'log'; line: LogLine }
  | { type: 'app.changed'; app: string }
  | { type: 'services.changed' }
  | { type: 'settings.changed' }
  | { type: 'event'; event: EventRecord };
