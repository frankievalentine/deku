import {
  mutationOptions,
  QueryClient,
  QueryClientProvider,
  type QueryClient as QueryClientType,
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query';
import { type ReactNode, useState } from 'react';
import {
  type App,
  addDomain,
  type ConfigVar,
  checkDaemonHealth,
  createApp,
  createManagedService,
  type Domain,
  deleteApp,
  deleteConfigVar,
  deleteManagedService,
  fetchApp,
  fetchAppRoutingStatus,
  fetchApps,
  fetchConfig,
  fetchDeployments,
  fetchDomains,
  fetchLetsEncryptConfig,
  fetchLogs,
  fetchManagedService,
  fetchManagedServiceLogs,
  fetchManagedServices,
  fetchObjectStoreConfig,
  fetchPlugins,
  fetchPorts,
  fetchProcesses,
  fetchRoutingStatus,
  fetchRoutingTable,
  fetchScale,
  fetchServiceBackups,
  fetchSshKeys,
  fetchTlsStatus,
  linkManagedService,
  type ManagedServiceDetail,
  type ManagedServiceKind,
  type ManagedServiceSummary,
  removeDomain,
  restoreServiceBackup,
  rotateDashboardToken,
  type ScaleMap,
  setConfigVar,
  setLetsEncryptConfig,
  setScale,
  triggerServiceBackup,
  unlinkManagedService,
} from './api';

export interface ServiceOverview {
  apps: App[];
  servicesByKind: Record<ManagedServiceKind, ManagedServiceSummary[]>;
}

export interface SettingsSummary {
  totalApps: number;
  tlsEmail: string | null;
  tlsConfigured: boolean;
  objectStoreConfigured: boolean;
  objectStoreProvider: string | null;
  sshKeyCount: number;
  pluginCount: number;
  totalServices: number;
}

export interface AppLogEntry {
  id: string;
  createdAt: string | null;
  message: string;
  eventType: string;
}

interface QueryHookOptions {
  enabled?: boolean;
  refetchInterval?: number | false;
}

interface QueryEnabledOnly {
  enabled?: boolean;
}

export const queryKeys = {
  apps: {
    list: ['apps', 'list'] as const,
    detailRoot: (appName: string) => ['apps', 'detail', appName] as const,
    summary: (appName: string) => ['apps', 'detail', appName, 'summary'] as const,
    deployments: (appName: string) => ['apps', 'detail', appName, 'deployments'] as const,
    domains: (appName: string) => ['apps', 'detail', appName, 'domains'] as const,
    ports: (appName: string) => ['apps', 'detail', appName, 'ports'] as const,
    config: (appName: string) => ['apps', 'detail', appName, 'config'] as const,
    scale: (appName: string) => ['apps', 'detail', appName, 'scale'] as const,
    processes: (appName: string) => ['apps', 'detail', appName, 'processes'] as const,
  },
  logs: {
    app: (appName: string, tailSize: number) => ['logs', 'apps', appName, tailSize] as const,
    service: (kind: ManagedServiceKind, name: string, tailSize: number) =>
      ['logs', 'services', kind, name, tailSize] as const,
  },
  routing: {
    table: ['routing', 'table'] as const,
    status: ['routing', 'status'] as const,
    app: (appName: string) => ['routing', 'app', appName] as const,
  },
  letsencrypt: {
    config: ['letsencrypt', 'config'] as const,
    app: (appName: string) => ['letsencrypt', 'app', appName] as const,
  },
  services: {
    overview: ['services', 'overview'] as const,
    list: (kind: ManagedServiceKind) => ['services', 'list', kind] as const,
    detail: (kind: ManagedServiceKind, name: string) => ['services', 'detail', kind, name] as const,
    backups: (kind: Extract<ManagedServiceKind, 'postgres' | 'redis'>, name: string) =>
      ['services', 'backups', kind, name] as const,
  },
  settings: {
    summary: ['settings', 'summary'] as const,
    health: ['settings', 'health'] as const,
  },
  sshKeys: {
    list: ['ssh-keys', 'list'] as const,
  },
  plugins: {
    list: ['plugins', 'list'] as const,
  },
  objectStore: {
    config: ['object-store', 'config'] as const,
  },
} as const;

let dashboardQueryClient: QueryClient | null = null;

function createDashboardQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: 1,
        refetchOnWindowFocus: false,
        refetchOnReconnect: true,
        staleTime: 10_000,
        gcTime: 300_000,
      },
    },
  });
}

export function getDashboardQueryClient(): QueryClient {
  if (!dashboardQueryClient) {
    dashboardQueryClient = createDashboardQueryClient();
  }

  return dashboardQueryClient;
}

export function DashboardQueryProvider({ children }: { children: ReactNode }) {
  const [queryClient] = useState(() => getDashboardQueryClient());

  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

export function getErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

export function getFirstQueryError(
  errors: Array<unknown>,
  fallback: string | null = null
): string | null {
  for (const error of errors) {
    if (error) {
      return getErrorMessage(error, fallback ?? 'Request failed.');
    }
  }

  return fallback;
}

export function appsListQueryOptions(options?: QueryHookOptions) {
  return queryOptions({
    queryKey: queryKeys.apps.list,
    queryFn: fetchApps,
    enabled: options?.enabled,
    refetchInterval: options?.refetchInterval,
  });
}

export function appSummaryQueryOptions(appName: string, options?: QueryEnabledOnly) {
  return queryOptions({
    queryKey: queryKeys.apps.summary(appName),
    queryFn: () => fetchApp(appName),
    enabled: options?.enabled ?? Boolean(appName),
  });
}

export function appDeploymentsQueryOptions(appName: string, options?: QueryHookOptions) {
  return queryOptions({
    queryKey: queryKeys.apps.deployments(appName),
    queryFn: () => fetchDeployments(appName),
    enabled: options?.enabled ?? Boolean(appName),
    refetchInterval: options?.refetchInterval,
  });
}

export function appDomainsQueryOptions(appName: string, options?: QueryEnabledOnly) {
  return queryOptions({
    queryKey: queryKeys.apps.domains(appName),
    queryFn: () => fetchDomains(appName),
    enabled: options?.enabled ?? Boolean(appName),
  });
}

export function appPortsQueryOptions(appName: string, options?: QueryEnabledOnly) {
  return queryOptions({
    queryKey: queryKeys.apps.ports(appName),
    queryFn: () => fetchPorts(appName),
    enabled: options?.enabled ?? Boolean(appName),
  });
}

export function appConfigQueryOptions(appName: string, options?: QueryEnabledOnly) {
  return queryOptions({
    queryKey: queryKeys.apps.config(appName),
    queryFn: () => fetchConfig(appName),
    enabled: options?.enabled ?? Boolean(appName),
  });
}

export function appScaleQueryOptions(appName: string, options?: QueryHookOptions) {
  return queryOptions({
    queryKey: queryKeys.apps.scale(appName),
    queryFn: () => fetchScale(appName),
    enabled: options?.enabled ?? Boolean(appName),
    refetchInterval: options?.refetchInterval,
  });
}

export function appProcessesQueryOptions(appName: string, options?: QueryHookOptions) {
  return queryOptions({
    queryKey: queryKeys.apps.processes(appName),
    queryFn: () => fetchProcesses(appName),
    enabled: options?.enabled ?? Boolean(appName),
    refetchInterval: options?.refetchInterval,
  });
}

export function routingTableQueryOptions(options?: QueryHookOptions) {
  return queryOptions({
    queryKey: queryKeys.routing.table,
    queryFn: fetchRoutingTable,
    enabled: options?.enabled,
    refetchInterval: options?.refetchInterval,
  });
}

export function routingStatusQueryOptions(options?: QueryHookOptions) {
  return queryOptions({
    queryKey: queryKeys.routing.status,
    queryFn: fetchRoutingStatus,
    enabled: options?.enabled,
    refetchInterval: options?.refetchInterval,
  });
}

export function appRoutingStatusQueryOptions(appName: string, options?: QueryEnabledOnly) {
  return queryOptions({
    queryKey: queryKeys.routing.app(appName),
    queryFn: () => fetchAppRoutingStatus(appName),
    enabled: options?.enabled ?? Boolean(appName),
  });
}

export function letsEncryptConfigQueryOptions(options?: QueryEnabledOnly) {
  return queryOptions({
    queryKey: queryKeys.letsencrypt.config,
    queryFn: fetchLetsEncryptConfig,
    enabled: options?.enabled,
  });
}

export function appTlsStatusQueryOptions(appName: string, options?: QueryEnabledOnly) {
  return queryOptions({
    queryKey: queryKeys.letsencrypt.app(appName),
    queryFn: () => fetchTlsStatus(appName),
    enabled: options?.enabled ?? Boolean(appName),
  });
}

export function servicesOverviewQueryOptions(options?: QueryEnabledOnly) {
  return queryOptions({
    queryKey: queryKeys.services.overview,
    queryFn: async (): Promise<ServiceOverview> => {
      const [apps, postgres, redis, mysql] = await Promise.all([
        fetchApps(),
        fetchManagedServices('postgres'),
        fetchManagedServices('redis'),
        fetchManagedServices('mysql'),
      ]);

      return {
        apps,
        servicesByKind: {
          postgres,
          redis,
          mysql,
        },
      };
    },
    enabled: options?.enabled,
  });
}

export function useManagedServiceDetailQuery(
  kind: ManagedServiceKind,
  name: string,
  options?: QueryEnabledOnly
) {
  return useQuery(managedServiceDetailQueryOptions(kind, name, options));
}

export function managedServiceDetailQueryOptions(
  kind: ManagedServiceKind,
  name: string,
  options?: QueryEnabledOnly
) {
  return queryOptions({
    queryKey: queryKeys.services.detail(kind, name),
    queryFn: () => fetchManagedService(kind, name),
    enabled: options?.enabled ?? Boolean(name),
  });
}

export function useManagedServiceBackupsQuery(
  kind: Extract<ManagedServiceKind, 'postgres' | 'redis'>,
  name: string,
  options?: QueryEnabledOnly
) {
  return useQuery(managedServiceBackupsQueryOptions(kind, name, options));
}

export function managedServiceBackupsQueryOptions(
  kind: Extract<ManagedServiceKind, 'postgres' | 'redis'>,
  name: string,
  options?: QueryEnabledOnly
) {
  return queryOptions({
    queryKey: queryKeys.services.backups(kind, name),
    queryFn: () => fetchServiceBackups(kind, name),
    enabled: options?.enabled ?? Boolean(name),
  });
}

export function useManagedServiceLogsQuery(
  kind: ManagedServiceKind,
  name: string,
  tailSize: number,
  options?: QueryEnabledOnly
) {
  return useQuery(managedServiceLogsQueryOptions(kind, name, tailSize, options));
}

export function managedServiceLogsQueryOptions(
  kind: ManagedServiceKind,
  name: string,
  tailSize: number,
  options?: QueryEnabledOnly
) {
  return queryOptions({
    queryKey: queryKeys.logs.service(kind, name, tailSize),
    queryFn: () => fetchManagedServiceLogs(kind, name, tailSize),
    enabled: options?.enabled ?? false,
  });
}

export function settingsSummaryQueryOptions(options?: QueryEnabledOnly) {
  return queryOptions({
    queryKey: queryKeys.settings.summary,
    queryFn: async (): Promise<SettingsSummary> => {
      const [apps, tls, objectStore, sshKeys, plugins, postgres, redis, mysql] = await Promise.all([
        fetchApps(),
        fetchLetsEncryptConfig(),
        fetchObjectStoreConfig(),
        fetchSshKeys(),
        fetchPlugins(),
        fetchManagedServices('postgres'),
        fetchManagedServices('redis'),
        fetchManagedServices('mysql'),
      ]);

      return {
        totalApps: apps.length,
        tlsEmail: tls.email,
        tlsConfigured: tls.configured,
        objectStoreConfigured: objectStore.configured,
        objectStoreProvider: objectStore.object_store?.provider ?? null,
        sshKeyCount: sshKeys.length,
        pluginCount: plugins.length,
        totalServices: postgres.length + redis.length + mysql.length,
      };
    },
    enabled: options?.enabled,
  });
}

export function daemonHealthQueryOptions(options?: QueryHookOptions) {
  return queryOptions({
    queryKey: queryKeys.settings.health,
    queryFn: () => checkDaemonHealth(),
    enabled: options?.enabled,
    refetchInterval: options?.refetchInterval,
  });
}

export function useAppLogsQuery(appName: string, tailSize: number, options?: QueryEnabledOnly) {
  return useQuery(appLogsQueryOptions(appName, tailSize, options));
}

export function appLogsQueryOptions(appName: string, tailSize: number, options?: QueryEnabledOnly) {
  return queryOptions({
    queryKey: queryKeys.logs.app(appName, tailSize),
    queryFn: async (): Promise<AppLogEntry[]> => {
      const lines = await fetchLogs(appName, tailSize);
      return lines.map((line, index) => ({
        id: `tail-${index}-${line}`,
        createdAt: null,
        eventType: 'log.tail',
        message: line,
      }));
    },
    enabled: options?.enabled ?? Boolean(appName),
  });
}

export function useAppsQuery(options?: QueryHookOptions) {
  return useQuery(appsListQueryOptions(options));
}

export function useAppSummaryQuery(appName: string, options?: QueryEnabledOnly) {
  return useQuery(appSummaryQueryOptions(appName, options));
}

export function useAppDeploymentsQuery(appName: string, options?: QueryHookOptions) {
  return useQuery(appDeploymentsQueryOptions(appName, options));
}

export function useAppDomainsQuery(appName: string, options?: QueryEnabledOnly) {
  return useQuery(appDomainsQueryOptions(appName, options));
}

export function useAppPortsQuery(appName: string, options?: QueryEnabledOnly) {
  return useQuery(appPortsQueryOptions(appName, options));
}

export function useAppConfigQuery(appName: string, options?: QueryEnabledOnly) {
  return useQuery(appConfigQueryOptions(appName, options));
}

export function useAppScaleQuery(appName: string, options?: QueryHookOptions) {
  return useQuery(appScaleQueryOptions(appName, options));
}

export function useAppProcessesQuery(appName: string, options?: QueryHookOptions) {
  return useQuery(appProcessesQueryOptions(appName, options));
}

export function useRoutingTableQuery(options?: QueryHookOptions) {
  return useQuery(routingTableQueryOptions(options));
}

export function useRoutingStatusQuery(options?: QueryHookOptions) {
  return useQuery(routingStatusQueryOptions(options));
}

export function useAppRoutingStatusQuery(appName: string, options?: QueryEnabledOnly) {
  return useQuery(appRoutingStatusQueryOptions(appName, options));
}

export function useLetsEncryptConfigQuery(options?: QueryEnabledOnly) {
  return useQuery(letsEncryptConfigQueryOptions(options));
}

export function useAppTlsStatusQuery(appName: string, options?: QueryEnabledOnly) {
  return useQuery(appTlsStatusQueryOptions(appName, options));
}

export function useServicesOverviewQuery(options?: QueryEnabledOnly) {
  return useQuery(servicesOverviewQueryOptions(options));
}

export function useSettingsSummaryQuery(options?: QueryEnabledOnly) {
  return useQuery(settingsSummaryQueryOptions(options));
}

export function useDaemonHealthQuery(options?: QueryHookOptions) {
  return useQuery(daemonHealthQueryOptions(options));
}

export async function prefetchAppDetail(
  queryClient: QueryClientType,
  appName: string
): Promise<void> {
  await Promise.all([
    queryClient.prefetchQuery(appSummaryQueryOptions(appName)),
    queryClient.prefetchQuery(appDeploymentsQueryOptions(appName)),
    queryClient.prefetchQuery(appDomainsQueryOptions(appName)),
    queryClient.prefetchQuery(appPortsQueryOptions(appName)),
    queryClient.prefetchQuery(appConfigQueryOptions(appName)),
    queryClient.prefetchQuery(appScaleQueryOptions(appName)),
    queryClient.prefetchQuery(appProcessesQueryOptions(appName)),
  ]);
}

export function useCreateAppMutation() {
  const queryClient = useQueryClient();

  return useMutation(createAppMutationOptions(queryClient));
}

export function createAppMutationOptions(queryClient: QueryClientType) {
  return mutationOptions({
    mutationFn: (name: string) => createApp(name),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.apps.list }),
        queryClient.invalidateQueries({ queryKey: queryKeys.settings.summary }),
      ]);
    },
  });
}

export function useDeleteAppMutation() {
  const queryClient = useQueryClient();

  return useMutation(deleteAppMutationOptions(queryClient));
}

export function deleteAppMutationOptions(queryClient: QueryClientType) {
  return mutationOptions({
    mutationFn: (name: string) => deleteApp(name),
    onSuccess: async (_result, appName) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.apps.list }),
        queryClient.removeQueries({ queryKey: queryKeys.apps.detailRoot(appName) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.settings.summary }),
        queryClient.invalidateQueries({ queryKey: queryKeys.routing.status }),
        queryClient.invalidateQueries({ queryKey: queryKeys.routing.table }),
      ]);
    },
  });
}

export function useAddDomainMutation(appName: string) {
  const queryClient = useQueryClient();

  return useMutation(addDomainMutationOptions(queryClient, appName));
}

export function addDomainMutationOptions(queryClient: QueryClientType, appName: string) {
  return mutationOptions({
    mutationFn: (domain: string) => addDomain(appName, domain),
    onSuccess: async () => {
      await invalidateAppRoutingQueries(queryClient, appName);
    },
  });
}

export function useRemoveDomainMutation(appName: string) {
  const queryClient = useQueryClient();

  return useMutation(removeDomainMutationOptions(queryClient, appName));
}

export function removeDomainMutationOptions(queryClient: QueryClientType, appName: string) {
  return mutationOptions({
    mutationFn: (domain: string) => removeDomain(appName, domain),
    onMutate: async (domain) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.apps.domains(appName) });
      const previousDomains = queryClient.getQueryData<Domain[]>(queryKeys.apps.domains(appName));

      queryClient.setQueryData<Domain[]>(queryKeys.apps.domains(appName), (current = []) =>
        current.filter((entry) => entry.domain !== domain)
      );

      return { previousDomains };
    },
    onError: (_error, _domain, context) => {
      if (context?.previousDomains) {
        queryClient.setQueryData(queryKeys.apps.domains(appName), context.previousDomains);
      }
    },
    onSuccess: async () => {
      await invalidateAppRoutingQueries(queryClient, appName);
    },
  });
}

export function useSetConfigVarMutation(appName: string) {
  const queryClient = useQueryClient();

  return useMutation(setConfigVarMutationOptions(queryClient, appName));
}

export function setConfigVarMutationOptions(queryClient: QueryClientType, appName: string) {
  return mutationOptions({
    mutationFn: ({ key, value }: { key: string; value: string }) =>
      setConfigVar(appName, key, value),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.apps.config(appName) });
    },
  });
}

export function useDeleteConfigVarMutation(appName: string) {
  const queryClient = useQueryClient();

  return useMutation(deleteConfigVarMutationOptions(queryClient, appName));
}

export function deleteConfigVarMutationOptions(queryClient: QueryClientType, appName: string) {
  return mutationOptions({
    mutationFn: (key: string) => deleteConfigVar(appName, key),
    onMutate: async (key) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.apps.config(appName) });
      const previousConfig = queryClient.getQueryData<ConfigVar[]>(queryKeys.apps.config(appName));

      queryClient.setQueryData<ConfigVar[]>(queryKeys.apps.config(appName), (current = []) =>
        current.filter((entry) => entry.key !== key)
      );

      return { previousConfig };
    },
    onError: (_error, _key, context) => {
      if (context?.previousConfig) {
        queryClient.setQueryData(queryKeys.apps.config(appName), context.previousConfig);
      }
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.apps.config(appName) });
    },
  });
}

export function useSetScaleMutation(appName: string) {
  const queryClient = useQueryClient();

  return useMutation(setScaleMutationOptions(queryClient, appName));
}

export function setScaleMutationOptions(queryClient: QueryClientType, appName: string) {
  return mutationOptions({
    mutationFn: (scales: ScaleMap) => setScale(appName, scales),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.apps.scale(appName) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.apps.processes(appName) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.routing.app(appName) }),
      ]);
    },
  });
}

export function useCreateManagedServiceMutation() {
  const queryClient = useQueryClient();

  return useMutation(createManagedServiceMutationOptions(queryClient));
}

export function createManagedServiceMutationOptions(queryClient: QueryClientType) {
  return mutationOptions({
    mutationFn: ({ kind, name }: { kind: ManagedServiceKind; name: string }) =>
      createManagedService(kind, name),
    onSuccess: async (_result, variables) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.services.overview }),
        queryClient.invalidateQueries({ queryKey: queryKeys.services.list(variables.kind) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.settings.summary }),
      ]);
    },
  });
}

export function useDeleteManagedServiceMutation() {
  const queryClient = useQueryClient();

  return useMutation(deleteManagedServiceMutationOptions(queryClient));
}

export function deleteManagedServiceMutationOptions(queryClient: QueryClientType) {
  return mutationOptions({
    mutationFn: ({ kind, name }: { kind: ManagedServiceKind; name: string }) =>
      deleteManagedService(kind, name),
    onMutate: async ({ kind, name }) => {
      await Promise.all([
        queryClient.cancelQueries({ queryKey: queryKeys.services.overview }),
        queryClient.cancelQueries({ queryKey: queryKeys.services.detail(kind, name) }),
      ]);

      const previousOverview = queryClient.getQueryData<ServiceOverview>(
        queryKeys.services.overview
      );
      const previousDetail = queryClient.getQueryData<ManagedServiceDetail>(
        queryKeys.services.detail(kind, name)
      );

      if (previousOverview) {
        queryClient.setQueryData<ServiceOverview>(queryKeys.services.overview, {
          ...previousOverview,
          servicesByKind: {
            ...previousOverview.servicesByKind,
            [kind]: previousOverview.servicesByKind[kind].filter(
              (service) => service.name !== name
            ),
          },
        });
      }

      queryClient.removeQueries({ queryKey: queryKeys.services.detail(kind, name) });
      queryClient.removeQueries({
        queryKey:
          kind === 'postgres' || kind === 'redis'
            ? queryKeys.services.backups(kind, name)
            : queryKeys.services.detail(kind, name),
      });

      return { previousOverview, previousDetail };
    },
    onError: (_error, variables, context) => {
      if (context?.previousOverview) {
        queryClient.setQueryData(queryKeys.services.overview, context.previousOverview);
      }
      if (context?.previousDetail) {
        queryClient.setQueryData(
          queryKeys.services.detail(variables.kind, variables.name),
          context.previousDetail
        );
      }
    },
    onSuccess: async (_result, variables) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.services.overview }),
        queryClient.invalidateQueries({ queryKey: queryKeys.services.list(variables.kind) }),
        queryClient.invalidateQueries({
          queryKey: queryKeys.services.detail(variables.kind, variables.name),
        }),
        queryClient.invalidateQueries({ queryKey: queryKeys.settings.summary }),
      ]);
    },
  });
}

export function useLinkManagedServiceMutation() {
  const queryClient = useQueryClient();

  return useMutation(linkManagedServiceMutationOptions(queryClient));
}

export function linkManagedServiceMutationOptions(queryClient: QueryClientType) {
  return mutationOptions({
    mutationFn: ({
      kind,
      serviceName,
      appName,
    }: {
      kind: ManagedServiceKind;
      serviceName: string;
      appName: string;
    }) => linkManagedService(kind, serviceName, appName),
    onSuccess: async (_result, variables) => {
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: queryKeys.services.detail(variables.kind, variables.serviceName),
        }),
        queryClient.invalidateQueries({ queryKey: queryKeys.services.overview }),
      ]);
    },
  });
}

export function useUnlinkManagedServiceMutation() {
  const queryClient = useQueryClient();

  return useMutation(unlinkManagedServiceMutationOptions(queryClient));
}

export function unlinkManagedServiceMutationOptions(queryClient: QueryClientType) {
  return mutationOptions({
    mutationFn: ({
      kind,
      serviceName,
      appName,
    }: {
      kind: ManagedServiceKind;
      serviceName: string;
      appName: string;
    }) => unlinkManagedService(kind, serviceName, appName),
    onMutate: async ({ kind, serviceName, appName }) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.services.detail(kind, serviceName) });
      const previousDetail = queryClient.getQueryData<ManagedServiceDetail>(
        queryKeys.services.detail(kind, serviceName)
      );

      queryClient.setQueryData<ManagedServiceDetail | undefined>(
        queryKeys.services.detail(kind, serviceName),
        (current) =>
          current
            ? {
                ...current,
                links: current.links.filter((link) => link.name !== appName),
              }
            : current
      );

      return { previousDetail };
    },
    onError: (_error, variables, context) => {
      if (context?.previousDetail) {
        queryClient.setQueryData(
          queryKeys.services.detail(variables.kind, variables.serviceName),
          context.previousDetail
        );
      }
    },
    onSuccess: async (_result, variables) => {
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: queryKeys.services.detail(variables.kind, variables.serviceName),
        }),
        queryClient.invalidateQueries({ queryKey: queryKeys.services.overview }),
      ]);
    },
  });
}

export function useTriggerServiceBackupMutation() {
  const queryClient = useQueryClient();

  return useMutation(triggerServiceBackupMutationOptions(queryClient));
}

export function triggerServiceBackupMutationOptions(queryClient: QueryClientType) {
  return mutationOptions({
    mutationFn: ({
      kind,
      name,
    }: {
      kind: Extract<ManagedServiceKind, 'postgres' | 'redis'>;
      name: string;
    }) => triggerServiceBackup(kind, name),
    onSuccess: async (_result, variables) => {
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: queryKeys.services.detail(variables.kind, variables.name),
        }),
        queryClient.invalidateQueries({
          queryKey: queryKeys.services.backups(variables.kind, variables.name),
        }),
      ]);
    },
  });
}

export function useRestoreServiceBackupMutation() {
  const queryClient = useQueryClient();

  return useMutation(restoreServiceBackupMutationOptions(queryClient));
}

export function restoreServiceBackupMutationOptions(queryClient: QueryClientType) {
  return mutationOptions({
    mutationFn: ({
      kind,
      name,
      backupId,
    }: {
      kind: Extract<ManagedServiceKind, 'postgres' | 'redis'>;
      name: string;
      backupId: string;
    }) => restoreServiceBackup(kind, name, backupId),
    onSuccess: async (_result, variables) => {
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: queryKeys.services.detail(variables.kind, variables.name),
        }),
        queryClient.invalidateQueries({
          queryKey: queryKeys.services.backups(variables.kind, variables.name),
        }),
      ]);
    },
  });
}

export function useSetLetsEncryptConfigMutation() {
  const queryClient = useQueryClient();

  return useMutation(setLetsEncryptConfigMutationOptions(queryClient));
}

export function setLetsEncryptConfigMutationOptions(queryClient: QueryClientType) {
  return mutationOptions({
    mutationFn: (email: string) => setLetsEncryptConfig(email),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.letsencrypt.config }),
        queryClient.invalidateQueries({ queryKey: queryKeys.settings.summary }),
        queryClient.invalidateQueries({ queryKey: queryKeys.routing.status }),
        queryClient.invalidateQueries({ queryKey: queryKeys.routing.table }),
      ]);
    },
  });
}

export function useRotateDashboardTokenMutation() {
  return useMutation(rotateDashboardTokenMutationOptions());
}

export function rotateDashboardTokenMutationOptions() {
  return mutationOptions({
    mutationFn: () => rotateDashboardToken(),
  });
}

export async function invalidateAppDetailQueries(
  queryClient: QueryClientType,
  appName: string
): Promise<void> {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: queryKeys.apps.detailRoot(appName) }),
    queryClient.invalidateQueries({ queryKey: queryKeys.routing.app(appName) }),
    queryClient.invalidateQueries({ queryKey: queryKeys.letsencrypt.app(appName) }),
    queryClient.invalidateQueries({ queryKey: queryKeys.apps.list }),
  ]);
}

export async function invalidateAppRoutingQueries(
  queryClient: QueryClientType,
  appName: string
): Promise<void> {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: queryKeys.apps.summary(appName) }),
    queryClient.invalidateQueries({ queryKey: queryKeys.apps.domains(appName) }),
    queryClient.invalidateQueries({ queryKey: queryKeys.apps.ports(appName) }),
    queryClient.invalidateQueries({ queryKey: queryKeys.routing.app(appName) }),
    queryClient.invalidateQueries({ queryKey: queryKeys.letsencrypt.app(appName) }),
    queryClient.invalidateQueries({ queryKey: queryKeys.routing.status }),
    queryClient.invalidateQueries({ queryKey: queryKeys.routing.table }),
    queryClient.invalidateQueries({ queryKey: queryKeys.apps.list }),
  ]);
}
