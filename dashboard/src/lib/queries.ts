import { QueryClient, queryOptions, useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { client } from '../api/client';
import type { Deployment, HostEvent, LogLine } from './types';

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // The host event stream invalidates what changes; nothing needs to poll or refetch on focus.
      staleTime: Number.POSITIVE_INFINITY,
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

export const keys = {
  apps: ['apps'] as const,
  app: (name: string) => ['apps', name] as const,
  deployments: (name: string) => ['apps', name, 'deployments'] as const,
  deployment: (name: string, id: string) => ['apps', name, 'deployments', id] as const,
  deploymentLogs: (name: string, id: string) => ['apps', name, 'deployments', id, 'logs'] as const,
  logs: (name: string) => ['apps', name, 'logs'] as const,
  config: (name: string) => ['apps', name, 'config'] as const,
  services: ['services'] as const,
  checks: ['host', 'checks'] as const,
  host: ['host', 'status'] as const,
  events: ['host', 'events'] as const,
  routes: ['host', 'routes'] as const,
  proxy: ['host', 'proxy'] as const,
  networks: ['host', 'networks'] as const,
  token: ['settings', 'token'] as const,
  acme: ['settings', 'acme'] as const,
  objectStore: ['settings', 'object-store'] as const,
  sshKeys: ['settings', 'ssh-keys'] as const,
  backups: (service: string) => ['services', service, 'backups'] as const,
  connection: (service: string) => ['services', service, 'connection'] as const,
  serviceLogs: (service: string) => ['services', service, 'logs'] as const,
};

export const q = {
  apps: () => queryOptions({ queryKey: keys.apps, queryFn: client.listApps }),
  app: (name: string) =>
    queryOptions({ queryKey: keys.app(name), queryFn: () => client.getApp(name) }),
  deployments: (name: string) =>
    queryOptions({ queryKey: keys.deployments(name), queryFn: () => client.listDeployments(name) }),
  deployment: (name: string, id: string) =>
    queryOptions({
      queryKey: keys.deployment(name, id),
      queryFn: () => client.getDeployment(name, id),
    }),
  deploymentLogs: (name: string, id: string) =>
    queryOptions({
      queryKey: keys.deploymentLogs(name, id),
      queryFn: () => client.deploymentLogs(name, id),
    }),
  logs: (name: string) =>
    queryOptions({ queryKey: keys.logs(name), queryFn: () => client.recentLogs(name) }),
  config: (name: string) =>
    queryOptions({ queryKey: keys.config(name), queryFn: () => client.listConfig(name) }),
  services: () => queryOptions({ queryKey: keys.services, queryFn: client.listServices }),
  checks: () => queryOptions({ queryKey: keys.checks, queryFn: client.listChecks }),
  host: () => queryOptions({ queryKey: keys.host, queryFn: client.getHost }),
  events: () => queryOptions({ queryKey: keys.events, queryFn: client.listEvents }),
  routes: () => queryOptions({ queryKey: keys.routes, queryFn: client.listRoutes }),
  proxy: () => queryOptions({ queryKey: keys.proxy, queryFn: client.getProxy }),
  networks: () => queryOptions({ queryKey: keys.networks, queryFn: client.listNetworks }),
  token: () => queryOptions({ queryKey: keys.token, queryFn: client.getToken }),
  acme: () => queryOptions({ queryKey: keys.acme, queryFn: client.getAcme }),
  objectStore: () => queryOptions({ queryKey: keys.objectStore, queryFn: client.getObjectStore }),
  sshKeys: () => queryOptions({ queryKey: keys.sshKeys, queryFn: client.listSshKeys }),
  backups: (service: string) =>
    queryOptions({ queryKey: keys.backups(service), queryFn: () => client.listBackups(service) }),
  connection: (service: string) =>
    queryOptions({
      queryKey: keys.connection(service),
      queryFn: () => client.serviceConnection(service),
    }),
  serviceLogs: (service: string) =>
    queryOptions({
      queryKey: keys.serviceLogs(service),
      queryFn: () => client.serviceLogs(service),
    }),
};

const LOG_CACHE_LINES = 1000;

function appendLine(lines: LogLine[] | undefined, line: LogLine): LogLine[] | undefined {
  if (!lines) return lines;
  const next = lines.length >= LOG_CACHE_LINES ? lines.slice(-LOG_CACHE_LINES + 1) : lines.slice();
  next.push(line);
  return next;
}

function upsertDeployment(
  list: Deployment[] | undefined,
  deployment: Deployment
): Deployment[] | undefined {
  if (!list) return list;
  const index = list.findIndex((entry) => entry.id === deployment.id);
  if (index === -1) return [deployment, ...list];
  const next = list.slice();
  next[index] = deployment;
  return next;
}

/**
 * Applies the host event stream to the query cache. Log lines and deploy progress are written
 * straight into the cache; anything coarser invalidates the queries it touches.
 */
/**
 * Follow an app's log lines into the cache while a screen shows them: runtime
 * output, or one deploy's build output when `deployment` is set.
 */
export function useLogStream(app: string, deployment: string | null = null): void {
  const cache = useQueryClient();
  useEffect(() => {
    const key = deployment ? keys.deploymentLogs(app, deployment) : keys.logs(app);
    return client.streamLogs(app, deployment, (line) => {
      cache.setQueryData<LogLine[]>(key, (lines) => {
        if (!lines || lines.some((existing) => existing.id === line.id)) return lines;
        return appendLine(lines, line);
      });
    });
  }, [app, deployment, cache]);
}

export function useHostEvents(): void {
  const cache = useQueryClient();
  useEffect(() => {
    return client.subscribe((event: HostEvent) => {
      switch (event.type) {
        case 'log': {
          const { line } = event;
          if (line.source === 'runtime') {
            cache.setQueryData<LogLine[]>(keys.logs(line.app), (lines) => appendLine(lines, line));
          } else if (line.deployment_id) {
            cache.setQueryData<LogLine[]>(
              keys.deploymentLogs(line.app, line.deployment_id),
              (lines) => appendLine(lines, line)
            );
          }
          break;
        }
        case 'deploy.status': {
          const { app, deployment } = event;
          cache.setQueryData<Deployment[]>(keys.deployments(app), (list) =>
            upsertDeployment(list, deployment)
          );
          cache.setQueryData(keys.deployment(app, deployment.id), deployment);
          void cache.invalidateQueries({ queryKey: keys.apps, exact: true });
          break;
        }
        case 'app.changed':
          void cache.invalidateQueries({ queryKey: keys.app(event.app), exact: true });
          void cache.invalidateQueries({ queryKey: keys.routes });
          void cache.invalidateQueries({ queryKey: keys.config(event.app) });
          // A prefix match: the list, each deploy, and each deploy's output.
          void cache.invalidateQueries({ queryKey: keys.deployments(event.app) });
          void cache.invalidateQueries({ queryKey: keys.apps, exact: true });
          break;
        case 'services.changed':
          void cache.invalidateQueries({ queryKey: keys.services });
          break;
        case 'settings.changed':
          void cache.invalidateQueries({ queryKey: ['settings'] });
          void cache.invalidateQueries({ queryKey: ['host'] });
          break;
        case 'resync':
          void cache.invalidateQueries();
          break;
        case 'event':
          cache.setQueryData(keys.events, (events: unknown) =>
            Array.isArray(events) ? [event.event, ...events].slice(0, 40) : events
          );
          break;
      }
    });
  }, [cache]);
}
