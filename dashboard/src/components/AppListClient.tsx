import { DashboardQueryProvider } from '../lib/query';
import AppList from './AppList';

export default function AppListClient() {
  return (
    <DashboardQueryProvider>
      <AppList />
    </DashboardQueryProvider>
  );
}
