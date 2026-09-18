const BASE_URL =
  typeof import.meta !== 'undefined' && import.meta.env?.PUBLIC_API_URL
    ? (import.meta.env.PUBLIC_API_URL as string)
    : '';

export const TOKEN_STORAGE_KEY = 'deku_token';
export const TOKEN_CHANGE_EVENT = 'deku:token-change';

export interface App {
  id: string;
  name: string;
  created_at: string;
  locked: boolean;
  status: 'created' | 'deployed' | 'stopped' | 'error';
  tls_enabled: boolean;
}

export interface Deployment {
  id: string;
  app_id: string;
  status:
    | 'pending'
    | 'building'
    | 'built'
    | 'deploying'
    | 'health_checking'
    | 'live'
    | 'failed'
    | 'rolled_back';
  builder: 'dockerfile' | 'nixpacks' | 'railpack' | 'pack' | 'image' | 'archive' | 'compose';
  image_tag: string | null;
  created_at: string;
  finished_at: string | null;
  /** The build's own URL, present while it is retained and a global domain is set. */
  preview_url?: string;
}

export interface ConfigVar {
  app_id: string;
  key: string;
  value: string;
  is_global: boolean;
  /** The stored value is ciphertext. */
  encrypted?: boolean;
  /** The value could not be decrypted, e.g. the key is not configured. */
  error?: string | null;
  /** Where the value came from: the app-wide set or an environment override. */
  source?: 'app' | 'environment' | string;
}

export interface Environment {
  id: string;
  app_id: string;
  name: string;
  slug: string;
  branch: string | null;
  is_production: boolean;
  created_at: string;
}

export interface Domain {
  id: string;
  app_id: string;
  domain: string;
  created_at: string;
}

export interface PortMapping {
  id: string;
  app_id: string;
  host_port: number;
  container_port: number;
  protocol: string;
}

export interface NetworkRecord {
  id: string;
  name: string;
}

export interface StorageMount {
  id: string;
  app_id: string;
  host_path: string;
  container_path: string;
}

export interface CronEntry {
  id: string;
  schedule: string;
  command: string;
}

export interface SshKey {
  id: string;
  name: string;
  fingerprint: string | null;
}

export interface Plugin {
  name: string;
  version: string | null;
  path: string;
}

export const MANAGED_SERVICE_KINDS = ['postgres', 'redis', 'mysql', 'mariadb', 'mongodb'] as const;

export type ManagedServiceKind = (typeof MANAGED_SERVICE_KINDS)[number];

export interface ManagedServiceSummary {
  id: string;
  name: string;
  status: string;
  plugin: string;
  container_id: string | null;
  created_at: string;
}

export interface ManagedServiceLink {
  name: string;
  env_key: string;
}

export type ManagedServiceConnection = Record<string, string | number | boolean | null>;

export interface ManagedServiceDetail {
  id: string;
  name: string;
  status: string;
  plugin: string;
  container_id: string | null;
  created_at: string;
  connection: ManagedServiceConnection;
  links: ManagedServiceLink[];
}

export interface ServiceBackup {
  id: string;
  service_id: string;
  object_key: string;
  format: string;
  size_bytes: number | null;
  sha256: string | null;
  created_at: string;
  restored_at: string | null;
  encryption: string;
}

export interface AlertRecord {
  id: string;
  rule: string;
  severity: 'warning' | 'critical';
  scope: string;
  subject: string;
  message: string;
  first_seen_at: string;
  last_seen_at: string;
  resolved_at: string | null;
}

export interface EventRecord {
  id: string;
  app_id: string | null;
  event_type: string;
  payload: string | null;
  created_at: string;
}

export interface Upstream {
  host: string;
  port: number;
}

/**
 * A hostname Angie serves for an app beyond the app's own domains: one per
 * environment and one per retained deployment.
 */
export interface DerivedHostname {
  hostname: string;
  environment: string;
  /** Set when the hostname pins a single deployment. */
  deployment_id?: string;
}

export interface RoutingTableEntry {
  app: string;
  domains: string[];
  hostnames: DerivedHostname[];
  upstreams: Upstream[];
}

export interface FileStatus {
  path: string;
  exists: boolean;
  modified_at: string | null;
}

export interface AngieConfigStatus {
  config_valid: boolean;
  validation_error?: string | null;
}

export interface RoutingAppStatus {
  app: string;
  status: string;
  domains: string[];
  hostnames: DerivedHostname[];
  upstreams: Upstream[];
  tls_enabled: boolean;
  proxy_config_path: string;
  proxy_config_present: boolean;
  certificate: FileStatus;
  private_key: FileStatus;
  tls_ready: boolean;
  issues: string[];
}

export interface RoutingStatusResponse {
  angie: AngieConfigStatus;
  apps: RoutingAppStatus[];
}

export interface SingleRoutingStatusResponse {
  angie: AngieConfigStatus;
  app: RoutingAppStatus;
}

export interface CertificateStatus {
  enabled: boolean;
  domains: string[];
  certificate: FileStatus;
  private_key: FileStatus;
  ready: boolean;
  not_before: string | null;
  not_after: string | null;
  subject: string | null;
  expires_at: string | null;
  days_remaining: number | null;
  lifecycle: CertificateLifecycle;
  inspection_error?: string | null;
}

export type CertificateLifecycle = 'ok' | 'expiring' | 'expired' | 'missing' | 'unknown';

export interface LetsEncryptConfig {
  configured: boolean;
  email: string | null;
}

export interface ObjectStoreConfig {
  provider: string;
  bucket: string;
  region: string;
  endpoint: string;
  access_key_id: string;
  secret_access_key: string;
  path_style: boolean;
  prefix?: string | null;
}

export interface ObjectStoreState {
  configured: boolean;
  object_store: ObjectStoreConfig | null;
}

export interface AppObjectStoreLink {
  provider: string;
  bucket: string;
  region: string;
  endpoint: string;
  prefix: string;
  path_style: boolean;
  secret_present: boolean;
  linked_keys: string[];
}

export interface AppObjectStoreState {
  app: string;
  configured: boolean;
  linked: boolean;
  link: AppObjectStoreLink | null;
}

export interface ProcessRecord {
  process_type: string;
  scale: number;
  status: string;
  container_id: string;
  deployment_id: string;
  host_port: number;
  created_at: string;
}

export type ScaleMap = Record<string, number>;

export interface VersionStatus {
  current_version: string;
  latest_version: string | null;
  update_available: boolean;
  status: 'ok' | 'error';
  error: string | null;
}

export function getToken(): string | null {
  if (typeof window === 'undefined') return null;
  return window.localStorage.getItem(TOKEN_STORAGE_KEY);
}

function notifyTokenChange(): void {
  if (typeof window === 'undefined') return;
  window.dispatchEvent(new Event(TOKEN_CHANGE_EVENT));
}

export function setToken(token: string): void {
  window.localStorage.setItem(TOKEN_STORAGE_KEY, token);
  notifyTokenChange();
}

export function clearToken(): void {
  window.localStorage.removeItem(TOKEN_STORAGE_KEY);
  notifyTokenChange();
}

export function getDashboardBuildVersion(): string | null {
  if (typeof document === 'undefined') return null;
  return document.body.dataset.dashboardBuildVersion ?? null;
}

async function apiFetch<T>(
  path: string,
  init: RequestInit = {},
  tokenOverride?: string
): Promise<T> {
  const token = tokenOverride ?? getToken();
  const headers = new Headers(init.headers ?? {});

  const isFormData = typeof FormData !== 'undefined' && init.body instanceof FormData;

  if (!headers.has('Content-Type') && init.body && !isFormData) {
    headers.set('Content-Type', 'application/json');
  }

  if (token) {
    headers.set('Authorization', `Bearer ${token}`);
  }

  const response = await fetch(`${BASE_URL}${path}`, { ...init, headers });
  if (!response.ok) {
    const text = await response.text().catch(() => '');

    // The daemon reports failures as {"error": "..."}. Show that message on its
    // own: a caller renders it to an operator, who should not have to read a
    // method, path, and status code to learn what went wrong.
    let message: string | null = null;
    try {
      const parsed: unknown = JSON.parse(text);
      if (
        parsed &&
        typeof parsed === 'object' &&
        'error' in parsed &&
        typeof (parsed as { error: unknown }).error === 'string'
      ) {
        message = (parsed as { error: string }).error;
      }
    } catch {
      // Not JSON; fall back to the transport detail below.
    }

    throw new Error(message ?? `${init.method ?? 'GET'} ${path} -> ${response.status}: ${text}`);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return response.json() as Promise<T>;
}

export async function verifyToken(token: string): Promise<boolean> {
  try {
    await apiFetch<void>('/api/dashboard/session', { method: 'POST' }, token);
    return true;
  } catch (error) {
    if (error instanceof Error && error.message.includes(' 401:')) {
      return false;
    }
    throw error;
  }
}

export async function checkDaemonHealth(tokenOverride?: string): Promise<boolean> {
  const token = tokenOverride ?? getToken();
  const headers = new Headers();

  if (token) {
    headers.set('Authorization', `Bearer ${token}`);
  }

  try {
    const response = await fetch(`${BASE_URL}/healthz`, { headers });
    return response.ok;
  } catch {
    return false;
  }
}

export function rotateDashboardToken(): Promise<{ token: string }> {
  return apiFetch<{ token: string }>('/api/dashboard/token', { method: 'POST' });
}

export function fetchVersionStatus(): Promise<VersionStatus> {
  return apiFetch<VersionStatus>('/api/version');
}

export function fetchApps(): Promise<App[]> {
  return apiFetch<App[]>('/api/apps');
}

export function fetchApp(name: string): Promise<App> {
  return apiFetch<App>(`/api/apps/${encodeURIComponent(name)}`);
}

export function createApp(name: string): Promise<App> {
  return apiFetch<App>('/api/apps', {
    method: 'POST',
    body: JSON.stringify({ name }),
  });
}

export function deleteApp(name: string): Promise<void> {
  return apiFetch<void>(`/api/apps/${encodeURIComponent(name)}`, { method: 'DELETE' });
}

export function fetchDeployments(appName: string): Promise<Deployment[]> {
  return apiFetch<Deployment[]>(`/api/apps/${encodeURIComponent(appName)}/deployments`);
}

export function fetchEnvironments(appName: string): Promise<Environment[]> {
  return apiFetch<Environment[]>(`/api/apps/${encodeURIComponent(appName)}/environments`);
}

export function triggerImageDeploy(
  appName: string,
  image: string,
  environment?: string
): Promise<{ message: string }> {
  const body: Record<string, string> = { source: 'image', image };
  if (environment) body.environment = environment;

  return apiFetch<{ message: string }>(`/api/apps/${encodeURIComponent(appName)}/deploy`, {
    method: 'POST',
    body: JSON.stringify(body),
  });
}

export function triggerArchiveDeploy(
  appName: string,
  archive: File,
  environment?: string
): Promise<{ message: string }> {
  const formData = new FormData();
  formData.append('archive', archive);
  const query = environment ? `?environment=${encodeURIComponent(environment)}` : '';

  return apiFetch<{ message: string }>(
    `/api/apps/${encodeURIComponent(appName)}/deploy/archive${query}`,
    {
      method: 'POST',
      body: formData,
    }
  );
}

export function triggerRollback(
  appName: string,
  deploymentId: string
): Promise<{ message: string }> {
  return apiFetch<{ message: string }>(`/api/apps/${encodeURIComponent(appName)}/rollback`, {
    method: 'POST',
    body: JSON.stringify({ deployment_id: deploymentId }),
  });
}

export function fetchConfig(appName: string, environment?: string): Promise<ConfigVar[]> {
  const query = environment ? `?environment=${encodeURIComponent(environment)}` : '';
  return apiFetch<ConfigVar[]>(`/api/apps/${encodeURIComponent(appName)}/config${query}`);
}

export function setConfigVar(
  appName: string,
  key: string,
  value: string,
  environment?: string
): Promise<void> {
  const body: Record<string, string> = { key, value };
  if (environment) body.environment = environment;

  return apiFetch<void>(`/api/apps/${encodeURIComponent(appName)}/config`, {
    method: 'POST',
    body: JSON.stringify(body),
  });
}

export function deleteConfigVar(appName: string, key: string, environment?: string): Promise<void> {
  const query = environment ? `?environment=${encodeURIComponent(environment)}` : '';
  return apiFetch<void>(
    `/api/apps/${encodeURIComponent(appName)}/config/${encodeURIComponent(key)}${query}`,
    {
      method: 'DELETE',
    }
  );
}

export function fetchDomains(appName: string): Promise<Domain[]> {
  return apiFetch<Domain[]>(`/api/apps/${encodeURIComponent(appName)}/domains`);
}

export function addDomain(appName: string, domain: string): Promise<Domain> {
  return apiFetch<Domain>(`/api/apps/${encodeURIComponent(appName)}/domains`, {
    method: 'POST',
    body: JSON.stringify({ domain }),
  });
}

export function removeDomain(appName: string, domain: string): Promise<void> {
  return apiFetch<void>(
    `/api/apps/${encodeURIComponent(appName)}/domains/${encodeURIComponent(domain)}`,
    {
      method: 'DELETE',
    }
  );
}

export function fetchPorts(appName: string): Promise<PortMapping[]> {
  return apiFetch<PortMapping[]>(`/api/apps/${encodeURIComponent(appName)}/ports`);
}

export function addPortMapping(
  appName: string,
  hostPort: number,
  containerPort: number,
  protocol = 'tcp'
): Promise<PortMapping> {
  return apiFetch<PortMapping>(`/api/apps/${encodeURIComponent(appName)}/ports`, {
    method: 'POST',
    body: JSON.stringify({
      host_port: hostPort,
      container_port: containerPort,
      protocol,
    }),
  });
}

export function removePortMapping(appName: string, portId: string): Promise<void> {
  return apiFetch<void>(
    `/api/apps/${encodeURIComponent(appName)}/ports/${encodeURIComponent(portId)}`,
    {
      method: 'DELETE',
    }
  );
}

export function fetchScale(appName: string): Promise<ScaleMap> {
  return apiFetch<ScaleMap>(`/api/apps/${encodeURIComponent(appName)}/scale`);
}

export function fetchProcesses(appName: string): Promise<ProcessRecord[]> {
  return apiFetch<ProcessRecord[]>(`/api/apps/${encodeURIComponent(appName)}/ps`);
}

export function setScale(appName: string, scales: ScaleMap): Promise<void> {
  return apiFetch<void>(`/api/apps/${encodeURIComponent(appName)}/scale`, {
    method: 'POST',
    body: JSON.stringify({ scales }),
  });
}

export interface LogLine {
  id: string;
  app_id: string;
  deployment_id: string | null;
  environment_id: string | null;
  source: 'build' | 'runtime' | string;
  stream: 'stdout' | 'stderr' | string;
  level: string;
  message: string;
  created_at: string;
  snippet?: string | null;
}

/** Filters shared by the log query and the live stream. */
export interface LogFilters {
  search?: string;
  source?: string;
  stream?: string;
  level?: string;
  deployment?: string;
}

function logQueryString(filters: LogFilters): string {
  const parts: string[] = [];
  for (const [key, value] of Object.entries(filters)) {
    if (typeof value === 'string' && value.trim()) {
      parts.push(`${key}=${encodeURIComponent(value.trim())}`);
    }
  }
  return parts.join('&');
}

export async function fetchLogs(
  appName: string,
  n = 100,
  filters: LogFilters = {}
): Promise<LogLine[]> {
  const query = logQueryString(filters);
  const response = await apiFetch<{ lines: LogLine[] }>(
    `/api/apps/${encodeURIComponent(appName)}/logs?n=${n}${query ? `&${query}` : ''}`
  );
  return response.lines;
}

export function appLogStreamUrl(appName: string, token?: string, filters: LogFilters = {}): string {
  const query = logQueryString(filters);
  const url = `${BASE_URL}/api/apps/${encodeURIComponent(appName)}/logs/stream${
    query ? `?${query}` : ''
  }`;
  if (!token) return url;
  return `${url}${query ? '&' : '?'}token=${encodeURIComponent(token)}`;
}

export function fetchSshKeys(): Promise<SshKey[]> {
  return apiFetch<SshKey[]>('/api/ssh-keys');
}

export function addSshKey(name: string, publicKey: string): Promise<SshKey> {
  return apiFetch<SshKey>('/api/ssh-keys', {
    method: 'POST',
    body: JSON.stringify({ name, public_key: publicKey }),
  });
}

export function deleteSshKey(name: string): Promise<void> {
  return apiFetch<void>(`/api/ssh-keys/${encodeURIComponent(name)}`, { method: 'DELETE' });
}

export function fetchPlugins(): Promise<Plugin[]> {
  return apiFetch<Plugin[]>('/api/plugins');
}

export function installPlugin(path: string): Promise<{ name: string }> {
  return apiFetch<{ name: string }>('/api/plugins', {
    method: 'POST',
    body: JSON.stringify({ path }),
  });
}

export function deletePlugin(name: string): Promise<void> {
  return apiFetch<void>(`/api/plugins/${encodeURIComponent(name)}`, { method: 'DELETE' });
}

export function fetchAlerts(): Promise<AlertRecord[]> {
  return apiFetch<AlertRecord[]>('/api/alerts');
}

export function fetchEvents(appId?: string): Promise<EventRecord[]> {
  const suffix = appId ? `?app=${encodeURIComponent(appId)}` : '';
  return apiFetch<EventRecord[]>(`/api/events${suffix}`);
}

export function appEventStreamUrl(appName: string, token?: string): string {
  const url = `${BASE_URL}/api/apps/${encodeURIComponent(appName)}/events/stream`;
  if (!token) return url;
  return `${url}?token=${encodeURIComponent(token)}`;
}

export function fetchNetworks(): Promise<NetworkRecord[]> {
  return apiFetch<NetworkRecord[]>('/api/networks');
}

export function createNetwork(name: string): Promise<NetworkRecord> {
  return apiFetch<NetworkRecord>('/api/networks', {
    method: 'POST',
    body: JSON.stringify({ name }),
  });
}

export function deleteNetwork(name: string): Promise<void> {
  return apiFetch<void>(`/api/networks/${encodeURIComponent(name)}`, {
    method: 'DELETE',
  });
}

export function fetchAppNetworks(appName: string): Promise<NetworkRecord[]> {
  return apiFetch<NetworkRecord[]>(`/api/apps/${encodeURIComponent(appName)}/networks`);
}

export function attachAppNetwork(appName: string, networkName: string): Promise<void> {
  return apiFetch<void>(
    `/api/apps/${encodeURIComponent(appName)}/networks/${encodeURIComponent(networkName)}`,
    {
      method: 'POST',
    }
  );
}

export function detachAppNetwork(appName: string, networkName: string): Promise<void> {
  return apiFetch<void>(
    `/api/apps/${encodeURIComponent(appName)}/networks/${encodeURIComponent(networkName)}`,
    {
      method: 'DELETE',
    }
  );
}

export function fetchStorageMounts(appName: string): Promise<StorageMount[]> {
  return apiFetch<StorageMount[]>(`/api/apps/${encodeURIComponent(appName)}/storage`);
}

export function addStorageMount(
  appName: string,
  hostPath: string,
  containerPath: string
): Promise<StorageMount> {
  return apiFetch<StorageMount>(`/api/apps/${encodeURIComponent(appName)}/storage`, {
    method: 'POST',
    body: JSON.stringify({
      host_path: hostPath,
      container_path: containerPath,
    }),
  });
}

export function removeStorageMount(appName: string, mountId: string): Promise<void> {
  return apiFetch<void>(
    `/api/apps/${encodeURIComponent(appName)}/storage/${encodeURIComponent(mountId)}`,
    {
      method: 'DELETE',
    }
  );
}

export function ensureStorageDirectory(appName: string, path: string): Promise<void> {
  return apiFetch<void>(`/api/apps/${encodeURIComponent(appName)}/storage/ensure`, {
    method: 'POST',
    body: JSON.stringify({ path }),
  });
}

export function fetchCronEntries(appName: string): Promise<CronEntry[]> {
  return apiFetch<CronEntry[]>(`/api/apps/${encodeURIComponent(appName)}/cron`);
}

export function addCronEntry(
  appName: string,
  schedule: string,
  command: string
): Promise<CronEntry> {
  return apiFetch<CronEntry>(`/api/apps/${encodeURIComponent(appName)}/cron`, {
    method: 'POST',
    body: JSON.stringify({ schedule, command }),
  });
}

export function removeCronEntry(appName: string, cronId: string): Promise<void> {
  return apiFetch<void>(
    `/api/apps/${encodeURIComponent(appName)}/cron/${encodeURIComponent(cronId)}`,
    {
      method: 'DELETE',
    }
  );
}

export function fetchRoutingTable(): Promise<RoutingTableEntry[]> {
  return apiFetch<RoutingTableEntry[]>('/api/routing');
}

export function fetchRoutingStatus(): Promise<RoutingStatusResponse> {
  return apiFetch<RoutingStatusResponse>('/api/routing/status');
}

export function fetchAppRoutingStatus(appName: string): Promise<SingleRoutingStatusResponse> {
  return apiFetch<SingleRoutingStatusResponse>(
    `/api/routing/status/${encodeURIComponent(appName)}`
  );
}

export function enableTls(appName: string): Promise<void> {
  return apiFetch<void>(`/api/letsencrypt/enable/${encodeURIComponent(appName)}`, {
    method: 'POST',
  });
}

export function disableTls(appName: string): Promise<void> {
  return apiFetch<void>(`/api/letsencrypt/disable/${encodeURIComponent(appName)}`, {
    method: 'POST',
  });
}

export function fetchTlsStatus(appName: string): Promise<CertificateStatus> {
  return apiFetch<CertificateStatus>(`/api/letsencrypt/status/${encodeURIComponent(appName)}`);
}

export function fetchLetsEncryptConfig(): Promise<LetsEncryptConfig> {
  return apiFetch<LetsEncryptConfig>('/api/letsencrypt/config');
}

export function setLetsEncryptConfig(email: string): Promise<void> {
  return apiFetch<void>('/api/letsencrypt/config', {
    method: 'POST',
    body: JSON.stringify({ email }),
  });
}

export interface AcmeSettings {
  enabled: boolean;
  directory: string;
  /** The host's certificate account email, set in its own section. */
  account_email: string | null;
  provider: string;
  wildcard: boolean;
  client_path: string;
  api_token_file: string | null;
  /** Whether a token is configured; the token itself is never returned. */
  token_configured: boolean;
  token_source: 'environment' | 'inline' | 'file' | null;
}

export interface AcmeSettingsInput {
  enabled: boolean;
  directory: string;
  provider: string;
  wildcard: boolean;
  /** A new token. Omit to keep the stored one. */
  api_token?: string;
}

export interface AcmeStatus extends AcmeSettings {
  /** The file that asks Angie for the certificate. */
  request_file: { path: string; written: boolean };
  certificate: {
    path: string;
    exists: boolean;
    expires_at: string | null;
    days_remaining: number | null;
    lifecycle: 'ok' | 'expiring' | 'expired' | 'missing' | 'unknown';
  };
}

export function fetchAcmeStatus(): Promise<AcmeStatus> {
  return apiFetch<AcmeStatus>('/api/acme/status');
}

export function fetchAcmeSettings(): Promise<AcmeSettings> {
  return apiFetch<AcmeSettings>('/api/acme');
}

export function saveAcmeSettings(input: AcmeSettingsInput): Promise<AcmeSettings> {
  return apiFetch<AcmeSettings>('/api/acme', {
    method: 'PUT',
    body: JSON.stringify(input),
  });
}

export function verifyAcmeToken(apiToken?: string): Promise<{ ok: boolean; zone_id: string }> {
  return apiFetch<{ ok: boolean; zone_id: string }>('/api/acme/verify', {
    method: 'POST',
    body: JSON.stringify({ api_token: apiToken ?? null }),
  });
}

export function fetchObjectStoreConfig(): Promise<ObjectStoreState> {
  return apiFetch<ObjectStoreState>('/api/objectstore');
}

export function fetchAppObjectStoreLink(appName: string): Promise<AppObjectStoreState> {
  return apiFetch<AppObjectStoreState>(`/api/apps/${encodeURIComponent(appName)}/objectstore`);
}

export function setObjectStoreConfig(config: ObjectStoreConfig): Promise<ObjectStoreState> {
  return apiFetch<ObjectStoreState>('/api/objectstore', {
    method: 'POST',
    body: JSON.stringify(config),
  });
}

export function linkAppObjectStore(
  appName: string,
  prefix?: string | null
): Promise<AppObjectStoreState> {
  return apiFetch<AppObjectStoreState>(`/api/apps/${encodeURIComponent(appName)}/objectstore`, {
    method: 'POST',
    body: JSON.stringify({ prefix: prefix || null }),
  });
}

export function unsetObjectStoreConfig(): Promise<void> {
  return apiFetch<void>('/api/objectstore', {
    method: 'DELETE',
  });
}

export function unlinkAppObjectStore(appName: string): Promise<void> {
  return apiFetch<void>(`/api/apps/${encodeURIComponent(appName)}/objectstore`, {
    method: 'DELETE',
  });
}

export function testObjectStoreConfig(): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>('/api/objectstore/test', {
    method: 'POST',
    body: JSON.stringify({}),
  });
}

function serviceBasePath(kind: ManagedServiceKind): string {
  return `/api/services/${kind}`;
}

export function fetchManagedServices(kind: ManagedServiceKind): Promise<ManagedServiceSummary[]> {
  return apiFetch<ManagedServiceSummary[]>(serviceBasePath(kind));
}

export function createManagedService(
  kind: ManagedServiceKind,
  name: string
): Promise<ManagedServiceSummary> {
  return apiFetch<ManagedServiceSummary>(serviceBasePath(kind), {
    method: 'POST',
    body: JSON.stringify({ name }),
  });
}

export function fetchManagedService(
  kind: ManagedServiceKind,
  name: string
): Promise<ManagedServiceDetail> {
  return apiFetch<ManagedServiceDetail>(`${serviceBasePath(kind)}/${encodeURIComponent(name)}`);
}

export function deleteManagedService(kind: ManagedServiceKind, name: string): Promise<void> {
  return apiFetch<void>(`${serviceBasePath(kind)}/${encodeURIComponent(name)}`, {
    method: 'DELETE',
  });
}

export function linkManagedService(
  kind: ManagedServiceKind,
  serviceName: string,
  appName: string
): Promise<void> {
  return apiFetch<void>(
    `${serviceBasePath(kind)}/${encodeURIComponent(serviceName)}/link/${encodeURIComponent(appName)}`,
    {
      method: 'POST',
    }
  );
}

export function unlinkManagedService(
  kind: ManagedServiceKind,
  serviceName: string,
  appName: string
): Promise<void> {
  return apiFetch<void>(
    `${serviceBasePath(kind)}/${encodeURIComponent(serviceName)}/link/${encodeURIComponent(appName)}`,
    {
      method: 'DELETE',
    }
  );
}

export async function fetchManagedServiceLogs(
  kind: ManagedServiceKind,
  name: string,
  n = 100
): Promise<string[]> {
  const response = await apiFetch<{ logs: string[] }>(
    `${serviceBasePath(kind)}/${encodeURIComponent(name)}/logs?n=${n}`
  );
  return response.logs;
}

export function fetchServiceBackups(
  kind: ManagedServiceKind,
  name: string
): Promise<ServiceBackup[]> {
  return apiFetch<ServiceBackup[]>(`${serviceBasePath(kind)}/${encodeURIComponent(name)}/backups`);
}

export function triggerServiceBackup(
  kind: ManagedServiceKind,
  name: string
): Promise<ServiceBackup> {
  return apiFetch<ServiceBackup>(`${serviceBasePath(kind)}/${encodeURIComponent(name)}/backups`, {
    method: 'POST',
  });
}

export function restoreServiceBackup(
  kind: ManagedServiceKind,
  name: string,
  backupId: string
): Promise<ServiceBackup> {
  return apiFetch<ServiceBackup>(
    `${serviceBasePath(kind)}/${encodeURIComponent(name)}/restore/${encodeURIComponent(backupId)}`,
    {
      method: 'POST',
    }
  );
}
