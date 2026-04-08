import type { IconName } from '../components/Icon';

export interface NavItem {
  href: string;
  label: string;
  id: string;
  icon: IconName;
  description: string;
}

export interface CommandItem {
  id: string;
  label: string;
  keywords: string[];
  kind: 'page' | 'action';
  icon: IconName;
  href?: string;
  shortcutKey?: string;
  action?:
    | 'open-create-app'
    | 'open-routing'
    | 'open-object-store'
    | 'open-settings'
    | 'rotate-token'
    | 'disconnect';
  description: string;
}

export interface ToastAction {
  label: string;
  href?: string;
  onclick?: string;
}

export interface ToastConfig {
  title?: string;
  description?: string;
  variant?: 'default' | 'success' | 'warning' | 'error';
  action?: ToastAction;
  duration?: number;
}

type ToastCategory = 'info' | 'success' | 'warning' | 'error';

interface ToastQueueWindow extends Window {
  __dekuToastQueue?: Array<{
    config: {
      category: ToastCategory;
      title?: string;
      description?: string;
      action?: ToastAction;
      duration?: number;
    };
  }>;
}

export const NAV_ITEMS: NavItem[] = [
  { href: '/', label: 'Apps', id: 'apps', icon: 'apps', description: 'Manage app fleet' },
  { href: '/host', label: 'Host', id: 'host', icon: 'host', description: 'View host overview' },
  {
    href: '/routing',
    label: 'Routing',
    id: 'routing',
    icon: 'routing',
    description: 'Inspect routes and TLS',
  },
  {
    href: '/services',
    label: 'Services',
    id: 'services',
    icon: 'services',
    description: 'Operate managed services',
  },
  {
    href: '/object-store',
    label: 'Object Store',
    id: 'object-store',
    icon: 'object-store',
    description: 'Configure object storage',
  },
  {
    href: '/ssh-keys',
    label: 'SSH Keys',
    id: 'ssh-keys',
    icon: 'ssh-keys',
    description: 'Manage SSH access',
  },
  {
    href: '/plugins',
    label: 'Plugins',
    id: 'plugins',
    icon: 'plugins',
    description: 'Review loaded plugins',
  },
  {
    href: '/settings',
    label: 'Settings',
    id: 'settings',
    icon: 'settings',
    description: 'Global dashboard settings',
  },
];

export function showToast({
  title,
  description,
  variant = 'default',
  action,
  duration = 5000,
}: ToastConfig): void {
  if (typeof document === 'undefined' || typeof window === 'undefined') return;

  const category: ToastCategory = variant === 'default' ? 'info' : variant;
  const detail = {
    config: {
      category,
      title,
      description,
      action,
      duration,
    },
  };

  if (!document.getElementById('toaster')) {
    const toastWindow = window as ToastQueueWindow;
    toastWindow.__dekuToastQueue = toastWindow.__dekuToastQueue ?? [];
    toastWindow.__dekuToastQueue.push(detail);
    return;
  }

  document.dispatchEvent(new CustomEvent('basecoat:toast', { detail }));
}

export function flushQueuedToasts(): void {
  if (typeof document === 'undefined' || typeof window === 'undefined') return;
  if (!document.getElementById('toaster')) return;

  const toastWindow = window as ToastQueueWindow;
  const queuedToasts = toastWindow.__dekuToastQueue ?? [];

  queuedToasts.forEach((detail) => {
    document.dispatchEvent(new CustomEvent('basecoat:toast', { detail }));
  });

  toastWindow.__dekuToastQueue = [];
}

export async function copyText(value: string): Promise<boolean> {
  if (typeof navigator === 'undefined' || !navigator.clipboard) {
    return false;
  }

  try {
    await navigator.clipboard.writeText(value);
    return true;
  } catch {
    return false;
  }
}
