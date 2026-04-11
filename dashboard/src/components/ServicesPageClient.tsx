import { DashboardQueryProvider } from '../lib/query';
import ServicesPage from './ServicesPage';

export default function ServicesPageClient() {
  return (
    <DashboardQueryProvider>
      <ServicesPage />
    </DashboardQueryProvider>
  );
}
