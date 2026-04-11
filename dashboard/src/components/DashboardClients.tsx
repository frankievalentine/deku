import type { ReactNode } from 'react';
import { DashboardQueryProvider } from '../lib/query';
import AppDetailPage from './AppDetailPage';
import AppList from './AppList';
import DeploymentDetailPage from './DeploymentDetailPage';
import HostPage from './HostPage';
import ObjectStorePage from './ObjectStorePage';
import PluginsPage from './PluginsPage';
import RoutingPage from './RoutingPage';
import ServicesPage from './ServicesPage';
import SettingsPage from './SettingsPage';
import SshKeysPage from './SshKeysPage';

function withDashboardQueryProvider(node: ReactNode) {
  return <DashboardQueryProvider>{node}</DashboardQueryProvider>;
}

export function AppsPageClient() {
  return withDashboardQueryProvider(<AppList />);
}

export function AppDetailPageClient() {
  return withDashboardQueryProvider(<AppDetailPage />);
}

export function DeploymentsPageClient() {
  return withDashboardQueryProvider(<DeploymentDetailPage />);
}

export function HostPageClient() {
  return withDashboardQueryProvider(<HostPage />);
}

export function RoutingPageClient() {
  return withDashboardQueryProvider(<RoutingPage />);
}

export function ServicesPageClient() {
  return withDashboardQueryProvider(<ServicesPage />);
}

export function ObjectStorePageClient() {
  return withDashboardQueryProvider(<ObjectStorePage />);
}

export function PluginsPageClient() {
  return withDashboardQueryProvider(<PluginsPage />);
}

export function SettingsPageClient() {
  return withDashboardQueryProvider(<SettingsPage />);
}

export function SshKeysPageClient() {
  return withDashboardQueryProvider(<SshKeysPage />);
}
