import { createRootRoute, createRoute, createRouter } from '@tanstack/react-router';
import { Shell } from './components/Shell';
import { AppConfig } from './pages/AppConfig';
import { AppDeploys } from './pages/AppDeploys';
import { AppLayout } from './pages/AppLayout';
import { AppLogs } from './pages/AppLogs';
import { AppOverview } from './pages/AppOverview';
import { AppSettings } from './pages/AppSettings';
import { AppsPage } from './pages/AppsPage';
import { DeployDetail } from './pages/DeployDetail';
import { HostPage } from './pages/HostPage';
import { NotFound } from './pages/NotFound';
import { ServicesPage } from './pages/ServicesPage';
import { SettingsPage } from './pages/SettingsPage';

const root = createRootRoute({ component: Shell, notFoundComponent: NotFound });

const apps = createRoute({ getParentRoute: () => root, path: '/', component: AppsPage });

const app = createRoute({ getParentRoute: () => root, path: 'apps/$app', component: AppLayout });
const appOverview = createRoute({ getParentRoute: () => app, path: '/', component: AppOverview });
const appDeploys = createRoute({
  getParentRoute: () => app,
  path: 'deploys',
  component: AppDeploys,
});
const deployDetail = createRoute({
  getParentRoute: () => app,
  path: 'deploys/$deployId',
  component: DeployDetail,
});

export interface LogSearch {
  q?: string;
  stream?: 'stdout' | 'stderr';
  process?: string;
}

const appLogs = createRoute({
  getParentRoute: () => app,
  path: 'logs',
  component: AppLogs,
  validateSearch: (search: Record<string, unknown>): LogSearch => ({
    q: typeof search.q === 'string' && search.q ? search.q : undefined,
    stream: search.stream === 'stdout' || search.stream === 'stderr' ? search.stream : undefined,
    process: typeof search.process === 'string' && search.process ? search.process : undefined,
  }),
});
const appConfig = createRoute({ getParentRoute: () => app, path: 'config', component: AppConfig });
const appSettings = createRoute({
  getParentRoute: () => app,
  path: 'settings',
  component: AppSettings,
});

const services = createRoute({
  getParentRoute: () => root,
  path: 'services',
  component: ServicesPage,
});
const host = createRoute({ getParentRoute: () => root, path: 'host', component: HostPage });
const settings = createRoute({
  getParentRoute: () => root,
  path: 'settings',
  component: SettingsPage,
});

const routeTree = root.addChildren([
  apps,
  app.addChildren([appOverview, appDeploys, deployDetail, appLogs, appConfig, appSettings]),
  services,
  host,
  settings,
]);

export const router = createRouter({
  routeTree,
  defaultPreload: 'intent',
  scrollRestoration: true,
});

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}
