import type { IconName } from '../components/Icon';

export interface NavItem {
  href: string;
  label: string;
  id: string;
  icon: IconName;
  description: string;
  external?: boolean;
}

export interface CommandItem {
  id: string;
  label: string;
  keywords: string[];
  kind: 'page' | 'action';
  icon: IconName;
  href?: string;
  external?: boolean;
  shortcutKey?: string;
  action?:
    | 'open-create-app'
    | 'open-routing'
    | 'open-storage'
    | 'open-settings'
    | 'rotate-token'
    | 'disconnect';
  description: string;
}

export interface ToastAction {
  label: string;
  href?: string;
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
      cancel?: { label: string };
      duration?: number;
    };
  }>;
}

export const NAV_ITEMS: NavItem[] = [
  { href: '/apps', label: 'Apps', id: 'apps', icon: 'apps', description: 'Apps and deployments' },
  {
    href: '/services',
    label: 'Services',
    id: 'services',
    icon: 'services',
    description: 'Databases and caches',
  },
  {
    href: '/storage',
    label: 'Storage',
    id: 'storage',
    icon: 'object-store',
    description: 'Backups and file storage',
  },
  {
    href: '/routing',
    label: 'Routing',
    id: 'routing',
    icon: 'routing',
    description: 'Domains and certificates',
  },
  {
    href: '/',
    label: 'Overview',
    id: 'host',
    icon: 'host',
    description: 'Server health and activity',
  },
  {
    href: '/access',
    label: 'SSH keys',
    id: 'access',
    icon: 'ssh-keys',
    description: 'Keys that can sign in',
  },
  {
    href: '/settings',
    label: 'Settings',
    id: 'settings',
    icon: 'settings',
    description: 'Access token and version',
  },
  {
    href: '/api/docs',
    label: 'API',
    id: 'api',
    icon: 'api',
    description: 'API reference',
    external: true,
  },
];

export function showToast({
  title,
  description,
  variant = 'default',
  action,
  duration,
}: ToastConfig): void {
  if (typeof document === 'undefined' || typeof window === 'undefined') return;

  const category: ToastCategory = variant === 'default' ? 'info' : variant;
  // Errors and toasts that carry an action stay until the user dismisses them.
  const persistent = category === 'error' || Boolean(action);
  const resolvedDuration = duration ?? (persistent ? -1 : 5000);
  const detail = {
    config: {
      category,
      title,
      description,
      action,
      cancel: persistent ? { label: 'Dismiss' } : undefined,
      duration: resolvedDuration,
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
