import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  checkDaemonHealth,
  clearToken,
  getToken,
  rotateDashboardToken,
  setToken,
  TOKEN_CHANGE_EVENT,
} from '../lib/api';
import { type CommandItem, flushQueuedToasts, NAV_ITEMS, showToast } from '../lib/shell';
import {
  applyTheme,
  getStoredThemeMode,
  THEME_CHANGE_EVENT,
  THEME_STORAGE_KEY,
  type ThemeMode,
} from '../lib/theme';
import Icon from './Icon';
import ThemeModeToggle from './ThemeModeToggle';

type ConnectionState = 'awaiting-token' | 'online' | 'offline';
const CONNECTION_STATE_STORAGE_KEY = 'deku_connection_state';
const PAGE_SHORTCUTS: Record<string, string> = {
  apps: 'a',
  host: 'h',
  routing: 'r',
  services: 's',
  'object-store': 'o',
  'ssh-keys': 'j',
  plugins: 'p',
  settings: ',',
};

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return Boolean(target.closest('input, textarea, select, [contenteditable="true"]'));
}

function formatShortcutLabel(shortcutKey: string, isApplePlatform: boolean): string {
  return isApplePlatform ? `⌘${shortcutKey.toUpperCase()}` : `Ctrl ${shortcutKey.toUpperCase()}`;
}

export default function ShellController() {
  const [headerRoot, setHeaderRoot] = useState<HTMLElement | null>(null);
  const [sidebarFooterRoot, setSidebarFooterRoot] = useState<HTMLElement | null>(null);
  const [connectionState, setConnectionState] = useState<ConnectionState>('awaiting-token');
  const [commandOpen, setCommandOpen] = useState(false);
  const [rotating, setRotating] = useState(false);
  const [hasToken, setHasToken] = useState(false);
  const [themeMode, setThemeMode] = useState<ThemeMode>('system');
  const [isApplePlatform, setIsApplePlatform] = useState(false);
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    setHeaderRoot(document.getElementById('shell-header-controls'));
    setSidebarFooterRoot(document.getElementById('shell-sidebar-footer-controls'));
  }, []);

  useEffect(() => {
    const token = getToken();
    const nextHasToken = Boolean(token);
    const storedConnectionState = nextHasToken
      ? window.sessionStorage.getItem(CONNECTION_STATE_STORAGE_KEY)
      : null;

    setHasToken(nextHasToken);
    setThemeMode(getStoredThemeMode());
    setIsApplePlatform(/(Mac|iPhone|iPad)/.test(navigator.userAgent));
    setConnectionState(
      storedConnectionState === 'online' || storedConnectionState === 'offline'
        ? storedConnectionState
        : 'awaiting-token'
    );
  }, []);

  useEffect(() => {
    flushQueuedToasts();
  }, []);

  useEffect(() => {
    let active = true;
    let timer: number | undefined;

    async function pollHealth() {
      const token = getToken();
      if (!token) {
        if (active) setConnectionState('awaiting-token');
        return;
      }

      const isOnline = await checkDaemonHealth(token);
      if (active) {
        setConnectionState(isOnline ? 'online' : 'offline');
      }
    }

    function schedule() {
      timer = window.setInterval(() => {
        void pollHealth();
      }, 30_000);
    }

    function handleTokenChange() {
      const token = getToken();
      const nextHasToken = Boolean(token);
      setHasToken(nextHasToken);
      if (!nextHasToken) {
        setConnectionState('awaiting-token');
        window.sessionStorage.removeItem(CONNECTION_STATE_STORAGE_KEY);
      }
      void pollHealth();
    }

    void pollHealth();
    schedule();
    window.addEventListener(TOKEN_CHANGE_EVENT, handleTokenChange);
    window.addEventListener('storage', handleTokenChange);

    return () => {
      active = false;
      if (timer) window.clearInterval(timer);
      window.removeEventListener(TOKEN_CHANGE_EVENT, handleTokenChange);
      window.removeEventListener('storage', handleTokenChange);
    };
  }, []);

  useEffect(() => {
    if (typeof window === 'undefined') return;

    if (connectionState === 'online' || connectionState === 'offline') {
      window.sessionStorage.setItem(CONNECTION_STATE_STORAGE_KEY, connectionState);
      return;
    }

    if (connectionState === 'awaiting-token') {
      window.sessionStorage.removeItem(CONNECTION_STATE_STORAGE_KEY);
    }
  }, [connectionState]);

  useEffect(() => {
    const media = window.matchMedia('(min-width: 1024px)');

    function syncSidebarLayout(event?: MediaQueryListEvent) {
      const isDesktop = event ? event.matches : media.matches;
      document.dispatchEvent(
        new CustomEvent('basecoat:sidebar', {
          detail: { id: 'dashboard-sidebar', action: isDesktop ? 'open' : 'close' },
        })
      );
    }

    syncSidebarLayout();
    media.addEventListener('change', syncSidebarLayout);

    return () => media.removeEventListener('change', syncSidebarLayout);
  }, []);

  useEffect(() => {
    function handleShortcut(event: KeyboardEvent) {
      if (!hasToken) return;
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
      if (isEditableTarget(event.target)) return;

      const key = event.key.toLowerCase();
      if (key === 'k') {
        event.preventDefault();
        setCommandOpen((open) => !open);
        return;
      }

      const matchedPage = NAV_ITEMS.find((item) => PAGE_SHORTCUTS[item.id] === key);
      if (matchedPage) {
        event.preventDefault();
        window.location.assign(matchedPage.href);
      }
    }

    window.addEventListener('keydown', handleShortcut);
    return () => window.removeEventListener('keydown', handleShortcut);
  }, [hasToken]);

  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');

    function syncThemeMode() {
      setThemeMode(getStoredThemeMode());
    }

    function handleMediaChange() {
      if (getStoredThemeMode() === 'system') {
        applyTheme('system', false);
      }
    }

    function handleStorage(event: StorageEvent) {
      if (event.key === THEME_STORAGE_KEY) {
        syncThemeMode();
      }
    }

    syncThemeMode();
    document.addEventListener(THEME_CHANGE_EVENT, syncThemeMode);
    window.addEventListener('storage', handleStorage);
    media.addEventListener('change', handleMediaChange);

    return () => {
      document.removeEventListener(THEME_CHANGE_EVENT, syncThemeMode);
      window.removeEventListener('storage', handleStorage);
      media.removeEventListener('change', handleMediaChange);
    };
  }, []);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;

    if (commandOpen && !dialog.open) {
      dialog.showModal();
      window.requestAnimationFrame(() => {
        inputRef.current?.focus();
      });
      return;
    }

    if (!commandOpen && dialog.open) {
      dialog.close();
    }
  }, [commandOpen]);

  useEffect(() => {
    if (!hasToken && commandOpen) {
      setCommandOpen(false);
    }
  }, [commandOpen, hasToken]);

  const commandItems = useMemo<CommandItem[]>(
    () => [
      ...NAV_ITEMS.map((item) => ({
        id: item.id,
        label: item.label,
        keywords: [item.label, item.id, item.description],
        kind: 'page' as const,
        icon: item.icon,
        href: item.href,
        shortcutKey: PAGE_SHORTCUTS[item.id],
        description: item.description,
      })),
      {
        id: 'create-app',
        label: 'Create app',
        keywords: ['new', 'app', 'fleet', 'create'],
        kind: 'action',
        icon: 'create',
        action: 'open-create-app',
        description: 'Open the app creation flow',
      },
      {
        id: 'open-routing',
        label: 'Open routing',
        keywords: ['routing', 'tls', 'domains'],
        kind: 'action',
        icon: 'routing',
        action: 'open-routing',
        description: 'Jump to routes and certificates',
      },
      {
        id: 'open-object-store',
        label: 'Open object store',
        keywords: ['object store', 'storage', 's3'],
        kind: 'action',
        icon: 'object-store',
        action: 'open-object-store',
        description: 'Jump to object storage configuration',
      },
      {
        id: 'open-settings',
        label: 'Open settings',
        keywords: ['settings', 'preferences', 'tls'],
        kind: 'action',
        icon: 'settings',
        action: 'open-settings',
        description: 'Jump to global dashboard settings',
      },
      {
        id: 'rotate-token',
        label: 'Rotate access token',
        keywords: ['security', 'token', 'rotate', 'dashboard'],
        kind: 'action',
        icon: 'rotate',
        action: 'rotate-token',
        description: 'Create a new dashboard token',
      },
      {
        id: 'disconnect',
        label: 'Disconnect dashboard',
        keywords: ['token', 'logout', 'disconnect'],
        kind: 'action',
        icon: 'disconnect',
        action: 'disconnect',
        description: 'Remove the stored dashboard token from this browser',
      },
    ],
    []
  );

  async function handleRotateToken() {
    if (!hasToken || rotating) {
      showToast({
        title: 'Dashboard token required',
        description: 'Connect with a valid dashboard token before rotating it.',
        variant: 'warning',
      });
      return;
    }

    const confirmed = window.confirm(
      'Rotate the dashboard token? Existing browser sessions will stop working.'
    );
    if (!confirmed) return;

    try {
      setRotating(true);
      const payload = await rotateDashboardToken();
      if (!payload?.token) {
        throw new Error('Daemon did not return a replacement token.');
      }

      setToken(payload.token);
      window.sessionStorage.setItem('deku_rotated_token', payload.token);
      showToast({
        title: 'Access token rotated',
        description: 'The new token is stored locally. Open Settings to copy it now.',
        variant: 'success',
        action: { label: 'Open Settings', href: '/settings' },
      });
    } catch (error) {
      showToast({
        title: 'Unable to rotate token',
        description: error instanceof Error ? error.message : 'Token rotation failed.',
        variant: 'error',
      });
    } finally {
      setRotating(false);
    }
  }

  function handleDisconnect() {
    if (!hasToken) {
      showToast({
        title: 'No token stored',
        description: 'This browser is already disconnected from the dashboard.',
        variant: 'warning',
      });
      return;
    }

    clearToken();
    window.sessionStorage.removeItem('deku_rotated_token');
    showToast({
      title: 'Logged out successfully',
      description: 'The stored dashboard token was removed from this browser.',
      variant: 'success',
    });
  }

  function handleCommandAction(item: CommandItem) {
    if (item.href) {
      window.location.assign(item.href);
      return;
    }

    switch (item.action) {
      case 'open-create-app':
        window.location.assign('/?create=1');
        break;
      case 'open-routing':
        window.location.assign('/routing');
        break;
      case 'open-object-store':
        window.location.assign('/object-store');
        break;
      case 'open-settings':
        window.location.assign('/settings');
        break;
      case 'rotate-token':
        void handleRotateToken();
        break;
      case 'disconnect':
        handleDisconnect();
        break;
      default:
        break;
    }
  }

  const headerControls = headerRoot
    ? createPortal(
        <HeaderControls
          commandOpen={commandOpen}
          connectionState={connectionState}
          hasToken={hasToken}
          isApplePlatform={isApplePlatform}
          onDisconnect={handleDisconnect}
          onOpenCommand={() => setCommandOpen(true)}
          onToggleSidebar={() => {
            document.dispatchEvent(
              new CustomEvent('basecoat:sidebar', {
                detail: { id: 'dashboard-sidebar', action: 'toggle' },
              })
            );
          }}
          onRotateToken={() => void handleRotateToken()}
          rotating={rotating}
        />,
        headerRoot
      )
    : null;

  const sidebarFooterControls = sidebarFooterRoot
    ? createPortal(
        <ThemeModeToggle
          mode={themeMode}
          compact
          onChange={(nextMode) => {
            setThemeMode(nextMode);
            applyTheme(nextMode);
          }}
        />,
        sidebarFooterRoot
      )
    : null;

  return (
    <>
      {headerControls}
      {sidebarFooterControls}

      <div
        id="toaster"
        className="toaster"
        data-align="end"
        aria-live="polite"
        aria-atomic="true"
      />

      {/* biome-ignore lint/a11y/useKeyWithClickEvents: Backdrop click is only used to dismiss the native dialog modal. */}
      <dialog
        ref={dialogRef}
        className="command-dialog"
        aria-label="Command menu"
        onClose={() => setCommandOpen(false)}
        onClick={(event) => {
          if (event.target === event.currentTarget) {
            setCommandOpen(false);
          }
        }}
      >
        <div className="command">
          <header>
            <Icon name="search" size={16} />
            <input
              ref={inputRef}
              id="dashboard-command-input"
              type="text"
              className="command-input"
              placeholder="Type a command or search..."
              autoComplete="off"
              autoCorrect="off"
              spellCheck={false}
              aria-autocomplete="list"
              role="combobox"
              aria-expanded="true"
              aria-controls="dashboard-command-menu"
              aria-label="Search dashboard"
            />
          </header>

          <div
            role="menu"
            id="dashboard-command-menu"
            className="scrollbar"
            aria-orientation="vertical"
            data-empty="No matching pages or actions."
          >
            {/* biome-ignore lint/a11y/useSemanticElements: Basecoat command uses ARIA group structure for menu sections. */}
            <div role="group" className="command-group" aria-labelledby="command-group-pages">
              {/* biome-ignore lint/a11y/useSemanticElements: Basecoat command uses ARIA heading rows inside menu groups. */}
              <span
                role="heading"
                aria-level={3}
                id="command-group-pages"
                className="command-group-title"
              >
                Pages
              </span>
              {commandItems
                .filter((item) => item.kind === 'page')
                .map((item) => (
                  <a
                    key={item.id}
                    id={`command-${item.id}`}
                    href={item.href}
                    role="menuitem"
                    className="no-underline"
                    data-filter={item.label}
                    data-keywords={item.keywords.join(' ')}
                  >
                    <Icon name={item.icon} size={16} />
                    <span>{item.label}</span>
                    {item.shortcutKey ? (
                      <kbd className="ml-auto bg-transparent text-muted-foreground tracking-widest">
                        {formatShortcutLabel(item.shortcutKey, isApplePlatform)}
                      </kbd>
                    ) : null}
                  </a>
                ))}
            </div>

            <hr />

            {/* biome-ignore lint/a11y/useSemanticElements: Basecoat command uses ARIA group structure for menu sections. */}
            <div role="group" className="command-group" aria-labelledby="command-group-actions">
              {/* biome-ignore lint/a11y/useSemanticElements: Basecoat command uses ARIA heading rows inside menu groups. */}
              <span
                role="heading"
                aria-level={3}
                id="command-group-actions"
                className="command-group-title"
              >
                Actions
              </span>
              {commandItems
                .filter((item) => item.kind === 'action')
                .map((item) => {
                  const disabled =
                    !hasToken &&
                    item.action !== 'open-create-app' &&
                    item.action !== 'open-settings';

                  return (
                    <button
                      key={item.id}
                      id={`command-${item.id}`}
                      type="button"
                      role="menuitem"
                      data-filter={item.label}
                      data-keywords={item.keywords.join(' ')}
                      disabled={disabled}
                      aria-disabled={disabled}
                      onClick={disabled ? undefined : () => handleCommandAction(item)}
                    >
                      <Icon name={item.icon} size={16} />
                      <span>{item.label}</span>
                      <kbd className="ml-auto bg-transparent text-muted-foreground tracking-widest">
                        ACTION
                      </kbd>
                    </button>
                  );
                })}
            </div>
          </div>

          <button type="button" aria-label="Close dialog" onClick={() => setCommandOpen(false)}>
            <Icon name="close" size={16} />
          </button>
        </div>
      </dialog>
    </>
  );
}

interface HeaderControlsProps {
  commandOpen: boolean;
  connectionState: ConnectionState;
  hasToken: boolean;
  isApplePlatform: boolean;
  rotating: boolean;
  onToggleSidebar: () => void;
  onOpenCommand: () => void;
  onRotateToken: () => void;
  onDisconnect: () => void;
}

function HeaderControls({
  commandOpen,
  connectionState,
  hasToken,
  isApplePlatform,
  rotating,
  onToggleSidebar,
  onOpenCommand,
  onRotateToken,
  onDisconnect,
}: HeaderControlsProps) {
  const shortcutLabel = isApplePlatform ? '⌘K' : 'Ctrl K';
  const label =
    connectionState === 'online'
      ? 'Online'
      : connectionState === 'offline'
        ? 'Offline'
        : 'Awaiting token';

  return (
    <div className="shell-header-actions">
      <button
        type="button"
        className="btn-sm-icon-outline shell-menu-trigger"
        onClick={onToggleSidebar}
        aria-label="Toggle navigation"
        aria-controls="dashboard-sidebar"
      >
        <Icon name="menu" size={16} />
      </button>

      <button
        type="button"
        className="btn-outline shell-search-trigger"
        onClick={onOpenCommand}
        disabled={!hasToken}
        aria-label="Search dashboard"
      >
        <span className="shell-search-label">
          <Icon name="search" size={16} />
          <span className="shell-search-text">Search</span>
        </span>
        <kbd className="kbd shell-command-shortcut">
          {shortcutLabel === '⌘K' ? (
            <>
              <Icon name="command" size={12} />
              <span>K</span>
            </>
          ) : (
            <span>{shortcutLabel}</span>
          )}
        </kbd>
      </button>

      <button
        type="button"
        className="btn-outline shell-action-button"
        onClick={onRotateToken}
        disabled={!hasToken}
        aria-label={rotating ? 'Rotating token' : 'Rotate token'}
      >
        <Icon name="rotate" size={16} className={rotating ? 'spin' : undefined} />
        {rotating ? (
          <>
            <span className="loading-spinner shell-action-spinner" />
            <span className="shell-action-label">Rotating</span>
          </>
        ) : (
          <span className="shell-action-label">Rotate token</span>
        )}
      </button>

      <button
        type="button"
        className="btn btn-danger shell-action-button"
        onClick={onDisconnect}
        disabled={!hasToken}
        aria-label="Disconnect dashboard"
      >
        <Icon name="disconnect" size={16} />
        <span className="shell-action-label">Disconnect</span>
      </button>

      <div
        className={`health-indicator ${commandOpen ? 'is-command-open' : ''}`}
        title="Daemon status"
      >
        <span className="health-dot" data-status={connectionState} />
        <span className="health-label">{label}</span>
      </div>
    </div>
  );
}
