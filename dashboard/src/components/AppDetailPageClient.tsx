import { DashboardQueryProvider } from '../lib/query';
import AppDetailPage from './AppDetailPage';

export default function AppDetailPageClient() {
  return (
    <DashboardQueryProvider>
      <AppDetailPage />
    </DashboardQueryProvider>
  );
}
