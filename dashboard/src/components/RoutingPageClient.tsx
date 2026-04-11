import { DashboardQueryProvider } from '../lib/query';
import RoutingPage from './RoutingPage';

export default function RoutingPageClient() {
  return (
    <DashboardQueryProvider>
      <RoutingPage />
    </DashboardQueryProvider>
  );
}
