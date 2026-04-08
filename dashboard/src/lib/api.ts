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
  builder: 'dockerfile' | 'nixpacks' | 'pack' | 'image' | 'archive' | 'compose';
  image_tag: string | null;
  created_at: string;
  finished_at: string | null;
}

export interface ConfigVar {
  app_id: string;
  key: string;
  value: string;
  is_global: boolean;
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

export type ManagedServiceKind = 'postgres' | 'redis' | 'mysql';

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

export interface RoutingTableEntry {
  app: string;
  domains: string[];
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
  inspection_error?: string | null;
}

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
    throw new Error(`${init.method ?? 'GET'} ${path} -> ${response.status}: ${text}`);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return response.json() as Promise<T>;
}

export async function verifyToken(token: string): Promise<boolean> {
  try {
    await apiFetch<App[]>('/api/apps', {}, token);
    return true;
  } catch {
    return false;
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

export function triggerImageDeploy(appName: string, image: string): Promise<{ message: string }> {
  return apiFetch<{ message: string }>(`/api/apps/${encodeURIComponent(appName)}/deploy`, {
    method: 'POST',
    body: JSON.stringify({ source: 'image', image }),
  });
}

export function triggerArchiveDeploy(appName: string, archive: File): Promise<{ message: string }> {
  const formData = new FormData();
  formData.append('archive', archive);

  return apiFetch<{ message: string }>(`/api/apps/${encodeURIComponent(appName)}/deploy/archive`, {
    method: 'POST',
    body: formData,
  });
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

export function fetchConfig(appName: string): Promise<ConfigVar[]> {
  return apiFetch<ConfigVar[]>(`/api/apps/${encodeURIComponent(appName)}/config`);
}

export function setConfigVar(appName: string, key: string, value: string): Promise<void> {
  return apiFetch<void>(`/api/apps/${encodeURIComponent(appName)}/config`, {
    method: 'POST',
    body: JSON.stringify({ key, value }),
  });
}

export function deleteConfigVar(appName: string, key: string): Promise<void> {
  return apiFetch<void>(
    `/api/apps/${encodeURIComponent(appName)}/config/${encodeURIComponent(key)}`,
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

export async function fetchLogs(appName: string, n = 100): Promise<string[]> {
  const response = await apiFetch<{ logs: string[] }>(
    `/api/apps/${encodeURIComponent(appName)}/logs?n=${n}`
  );
  return response.logs;
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
  return `/api/${kind}/services`;
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
  kind: Extract<ManagedServiceKind, 'postgres' | 'redis'>,
  name: string
): Promise<ServiceBackup[]> {
  return apiFetch<ServiceBackup[]>(`${serviceBasePath(kind)}/${encodeURIComponent(name)}/backups`);
}

export function triggerServiceBackup(
  kind: Extract<ManagedServiceKind, 'postgres' | 'redis'>,
  name: string
): Promise<ServiceBackup> {
  return apiFetch<ServiceBackup>(`${serviceBasePath(kind)}/${encodeURIComponent(name)}/backups`, {
    method: 'POST',
  });
}

export function restoreServiceBackup(
  kind: Extract<ManagedServiceKind, 'postgres' | 'redis'>,
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
