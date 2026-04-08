import {
  ArrowRight,
  Cloud,
  Command,
  Copy,
  Database,
  KeyRound,
  LayoutDashboard,
  LogOut,
  Menu,
  Monitor,
  MoonStar,
  Package,
  PanelLeft,
  PlugZap,
  Plus,
  RefreshCw,
  Router,
  Search,
  Server,
  Settings2,
  ShieldCheck,
  SunMedium,
  X,
} from 'lucide-react';
import type { ComponentType, SVGProps } from 'react';

export type IconName =
  | 'apps'
  | 'host'
  | 'routing'
  | 'services'
  | 'object-store'
  | 'ssh-keys'
  | 'plugins'
  | 'settings'
  | 'search'
  | 'menu'
  | 'theme-light'
  | 'theme-dark'
  | 'theme-system'
  | 'rotate'
  | 'disconnect'
  | 'copy'
  | 'create'
  | 'open'
  | 'command'
  | 'security'
  | 'sidebar'
  | 'close';

const ICONS: Record<IconName, ComponentType<SVGProps<SVGSVGElement>>> = {
  apps: LayoutDashboard,
  host: Server,
  routing: Router,
  services: Database,
  'object-store': Cloud,
  'ssh-keys': KeyRound,
  plugins: PlugZap,
  settings: Settings2,
  search: Search,
  menu: Menu,
  'theme-light': SunMedium,
  'theme-dark': MoonStar,
  'theme-system': Monitor,
  rotate: RefreshCw,
  disconnect: LogOut,
  copy: Copy,
  create: Plus,
  open: ArrowRight,
  command: Command,
  security: ShieldCheck,
  sidebar: PanelLeft,
  close: X,
};

interface IconProps extends SVGProps<SVGSVGElement> {
  name: IconName;
  size?: number;
}

export default function Icon({ name, size = 18, ...props }: IconProps) {
  const Component = ICONS[name] ?? Package;
  return <Component width={size} height={size} aria-hidden="true" {...props} />;
}
