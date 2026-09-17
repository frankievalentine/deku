type ToastToaster = HTMLElement & { toast?: (config: unknown) => void };
type SidebarElement = HTMLElement & {
  open?: () => void;
  close?: () => void;
  toggle?: () => void;
};

const SIDEBAR_ACTIONS = {
  open: (sidebar: SidebarElement) => sidebar.open?.(),
  close: (sidebar: SidebarElement) => sidebar.close?.(),
  toggle: (sidebar: SidebarElement) => sidebar.toggle?.(),
} as const;

type SidebarAction = keyof typeof SIDEBAR_ACTIONS;

const TOAST_CATEGORIES = ['info', 'success', 'warning', 'error'] as const;
type ToastCategory = (typeof TOAST_CATEGORIES)[number];

const SAFE_HREF_PROTOCOLS = new Set(['http:', 'https:', 'mailto:', 'tel:']);

export interface ToastActionInput {
  readonly label?: unknown;
  readonly href?: unknown;
}

/**
 * Untrusted shape of a `basecoat:toast` detail. Basecoat writes these strings
 * straight into `innerHTML`, and treats `onclick` and `icon` as raw code and
 * markup, so those two are declared only to be dropped.
 */
export interface BasecoatToastConfigInput {
  readonly category?: unknown;
  readonly title?: unknown;
  readonly description?: unknown;
  readonly duration?: unknown;
  readonly action?: ToastActionInput;
  readonly cancel?: ToastActionInput;
  readonly onclick?: unknown;
  readonly icon?: unknown;
}

interface SafeToastAction {
  readonly label: string;
  readonly href?: string;
}

export interface SafeToastConfig {
  readonly category: ToastCategory;
  readonly title?: string;
  readonly description?: string;
  readonly duration?: number;
  readonly action?: SafeToastAction;
  readonly cancel?: SafeToastAction;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function escapeText(value: unknown): string | undefined {
  if (typeof value !== 'string' || value.length === 0) return undefined;
  return escapeHtml(value);
}

function toDuration(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function isToastCategory(value: unknown): value is ToastCategory {
  return typeof value === 'string' && (TOAST_CATEGORIES as readonly string[]).includes(value);
}

function safeHref(value: unknown, baseUrl: string): string | undefined {
  if (typeof value !== 'string' || value.length === 0) return undefined;

  try {
    if (!SAFE_HREF_PROTOCOLS.has(new URL(value, baseUrl).protocol)) return undefined;
  } catch {
    return undefined;
  }

  return escapeHtml(value);
}

function sanitizeAction(
  input: ToastActionInput | undefined,
  baseUrl: string
): SafeToastAction | undefined {
  const label = escapeText(input?.label);
  if (label === undefined) return undefined;

  const href = safeHref(input?.href, baseUrl);
  return href === undefined ? { label } : { label, href };
}

/**
 * Escapes toast text, drops inline handlers and icon markup, and rejects
 * non-web href protocols before a config reaches Basecoat's HTML template.
 */
export function sanitizeToastConfig(
  raw: BasecoatToastConfigInput | undefined,
  baseUrl: string
): SafeToastConfig {
  const title = escapeText(raw?.title);
  const description = escapeText(raw?.description);
  const duration = toDuration(raw?.duration);
  const action = sanitizeAction(raw?.action, baseUrl);
  const cancel = sanitizeAction(raw?.cancel, baseUrl);

  return {
    category: isToastCategory(raw?.category) ? raw.category : 'info',
    ...(title === undefined ? {} : { title }),
    ...(description === undefined ? {} : { description }),
    ...(duration === undefined ? {} : { duration }),
    ...(action === undefined ? {} : { action }),
    ...(cancel === undefined ? {} : { cancel }),
  };
}

function whenInitialized<T extends HTMLElement>(element: T, run: (element: T) => void): void {
  element.addEventListener('basecoat:initialized', () => run(element), { once: true });
}

function withToaster(run: (toaster: ToastToaster) => void): void {
  const toaster = document.getElementById('toaster') as ToastToaster | null;
  if (!toaster) return;

  if (typeof toaster.toast === 'function') {
    run(toaster);
    return;
  }

  whenInitialized(toaster, run);
}

/**
 * Basecoat 1.0 replaced the document-level `basecoat:toast` and `basecoat:sidebar`
 * events with element methods. Translate the events this app dispatches so the
 * existing call sites keep working.
 */
export function installBasecoatEventBridges(): void {
  document.addEventListener('basecoat:toast', (event) => {
    const detail = (event as CustomEvent<{ config?: BasecoatToastConfigInput }>).detail;
    const config = sanitizeToastConfig(detail?.config, window.location.origin);
    withToaster((toaster) => toaster.toast?.(config));
  });

  document.addEventListener('basecoat:sidebar', (event) => {
    const { id, action } =
      (event as CustomEvent<{ id?: string; action?: SidebarAction }>).detail ?? {};
    if (!id || !action) return;

    const sidebar = document.getElementById(id) as SidebarElement | null;
    if (!sidebar) return;

    const run = () => SIDEBAR_ACTIONS[action](sidebar);
    if (typeof sidebar.toggle === 'function') {
      run();
      return;
    }

    whenInitialized(sidebar, run);
  });
}
