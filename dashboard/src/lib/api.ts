const BASE_URL =
  typeof import.meta !== 'undefined' && import.meta.env?.PUBLIC_API_URL
    ? (import.meta.env.PUBLIC_API_URL as string)
    : '';

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

export interface EventRecord {
  id: string;
  app_id: string | null;
  event_type: string;
  payload: string | null;
  created_at: string;
}

export type ScaleMap = Record<string, number>;

export function getToken(): string | null {
  if (typeof window === 'undefined') return null;
  return window.localStorage.getItem('deku_token');
}

export function setToken(token: string): void {
  window.localStorage.setItem('deku_token', token);
}

export function clearToken(): void {
  window.localStorage.removeItem('deku_token');
}

async function apiFetch<T>(
  path: string,
  init: RequestInit = {},
  tokenOverride?: string
): Promise<T> {
  const token = tokenOverride ?? getToken();
  const headers = new Headers(init.headers ?? {});

  if (!headers.has('Content-Type') && init.body) {
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

export function fetchScale(appName: string): Promise<ScaleMap> {
  return apiFetch<ScaleMap>(`/api/apps/${encodeURIComponent(appName)}/scale`);
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

export function fetchEvents(appId: string): Promise<EventRecord[]> {
  return apiFetch<EventRecord[]>(`/api/events?app=${encodeURIComponent(appId)}`);
}

export function appEventStreamUrl(appName: string, token?: string): string {
  const url = `${BASE_URL}/api/apps/${encodeURIComponent(appName)}/events/stream`;
  if (!token) return url;
  return `${url}?token=${encodeURIComponent(token)}`;
}
