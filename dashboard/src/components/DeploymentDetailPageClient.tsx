import { DashboardQueryProvider } from '../lib/query';
import DeploymentDetailPage from './DeploymentDetailPage';

export default function DeploymentDetailPageClient() {
  return (
    <DashboardQueryProvider>
      <DeploymentDetailPage />
    </DashboardQueryProvider>
  );
}
