type SidebarController = HTMLElement & { close?: () => void };

const MOBILE_QUERY = '(max-width: 1023px)';
const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(', ');

function focusableElements(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (element) => element.offsetParent !== null
  );
}

export function installMobileSidebarOverlay(): () => void {
  const sidebar = document.getElementById('dashboard-sidebar');
  const background = document.querySelector<HTMLElement>('.shell-body');

  if (!(sidebar instanceof HTMLElement) || !(background instanceof HTMLElement)) {
    return () => {};
  }

  const sidebarElement = sidebar;
  const backgroundElement = background;
  const media = window.matchMedia(MOBILE_QUERY);
  const closeButtons = Array.from(
    sidebarElement.querySelectorAll<HTMLElement>('.sidebar-close-button')
  );
  let trapped = false;
  let focusMoved = false;

  function closeSidebar() {
    (sidebarElement as SidebarController).close?.();
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      closeSidebar();
      return;
    }

    if (event.key !== 'Tab') return;

    const items = focusableElements(sidebarElement);
    if (items.length === 0) return;

    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;

    if (!(active instanceof HTMLElement) || !sidebarElement.contains(active)) {
      event.preventDefault();
      first.focus();
      return;
    }

    if (event.shiftKey && active === first) {
      event.preventDefault();
      last.focus();
      return;
    }

    if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function syncOverlay() {
    const shouldTrap = media.matches && sidebarElement.getAttribute('aria-hidden') === 'false';
    if (shouldTrap === trapped) return;

    trapped = shouldTrap;

    if (shouldTrap) {
      backgroundElement.inert = true;
      sidebarElement.addEventListener('keydown', handleKeyDown);
      const closeButton = sidebarElement.querySelector<HTMLElement>('.sidebar-close-button');
      const target = closeButton ?? focusableElements(sidebarElement)[0];
      if (target) {
        target.focus();
        focusMoved = true;
      }
      return;
    }

    backgroundElement.inert = false;
    sidebarElement.removeEventListener('keydown', handleKeyDown);

    if (focusMoved && media.matches) {
      document.querySelector<HTMLElement>('.shell-menu-trigger')?.focus();
    }

    focusMoved = false;
  }

  const observer = new MutationObserver(syncOverlay);
  observer.observe(sidebarElement, { attributeFilter: ['aria-hidden'] });
  media.addEventListener('change', syncOverlay);
  for (const button of closeButtons) button.addEventListener('click', closeSidebar);
  syncOverlay();

  return () => {
    observer.disconnect();
    media.removeEventListener('change', syncOverlay);
    for (const button of closeButtons) button.removeEventListener('click', closeSidebar);
    sidebarElement.removeEventListener('keydown', handleKeyDown);
    backgroundElement.inert = false;
  };
}
