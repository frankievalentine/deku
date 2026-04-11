import { DashboardQueryProvider } from '../lib/query';
import SettingsPage from './SettingsPage';

export default function SettingsPageClient() {
  return (
    <DashboardQueryProvider>
      <SettingsPage />
    </DashboardQueryProvider>
  );
}
