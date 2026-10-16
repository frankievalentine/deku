import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, Outlet, useRouterState } from '@tanstack/react-router';
import {
  Boxes,
  Database,
  Menu,
  Monitor,
  Moon,
  Search,
  Server,
  Settings,
  Sun,
  X,
} from 'lucide-react';
import { type ReactNode, useEffect, useState, useSyncExternalStore } from 'react';
import { client } from '../api/client';
import { SIGNED_OUT_EVENT } from '../api/http';
import { setPrefs, type ThemeMode, usePrefs } from '../lib/prefs';
import { q, useHostEvents } from '../lib/queries';
import { CommandPalette, openPalette } from './CommandPalette';
import { SignIn } from './SignIn';
import { Dot, Kbd, Toaster } from './ui';

export function Shell() {
  const session = useQuery({ queryKey: ['session'], queryFn: client.session, retry: false });
  const cache = useQueryClient();

  // Any request that finds the session gone sends the browser back here.
  useEffect(() => {
    const signedOut = () => cache.setQueryData(['session'], null);
    window.addEventListener(SIGNED_OUT_EVENT, signedOut);
    return () => window.removeEventListener(SIGNED_OUT_EVENT, signedOut);
  }, [cache]);

  if (session.isPending) return <div className="min-h-dvh bg-bg" />;
  if (!session.data) {
    return (
      <SignIn
        onSignedIn={() => {
          cache.clear();
          void session.refetch();
        }}
      />
    );
  }
  return <SignedInShell />;
}

function SignedInShell() {
  useHostEvents();
  const [menuOpen, setMenuOpen] = useState(false);
  const pathname = useRouterState({ select: (state) => state.location.pathname });

  // A navigation closes the mobile menu.
  // biome-ignore lint/correctness/useExhaustiveDependencies: pathname is the trigger, not an input
  useEffect(() => setMenuOpen(false), [pathname]);

  return (
    <div className="min-h-dvh bg-bg text-ink lg:grid lg:grid-cols-[232px_minmax(0,1fr)]">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:bg-surface focus:px-3 focus:py-2"
      >
        Skip to content
      </a>
      <aside
        className={`fixed inset-y-0 left-0 z-40 w-[264px] border-r border-line bg-bg transition-transform lg:sticky lg:top-0 lg:z-auto lg:h-dvh lg:w-auto lg:translate-x-0 ${
          menuOpen ? 'translate-x-0 shadow-2xl' : '-translate-x-full'
        }`}
      >
        <Sidebar onClose={() => setMenuOpen(false)} />
      </aside>
      {menuOpen ? (
        <button
          type="button"
          aria-label="Close menu"
          className="fixed inset-0 z-30 bg-black/30 lg:hidden"
          onClick={() => setMenuOpen(false)}
        />
      ) : null}

      <div className="flex min-w-0 flex-col">
        <TopBar onMenu={() => setMenuOpen(true)} />
        <main id="main" className="min-w-0 flex-1 px-4 pt-5 pb-16 sm:px-6 lg:px-8">
          <Outlet />
        </main>
      </div>
      <CommandPalette />
      <Toaster />
    </div>
  );
}

function Wordmark() {
  return (
    <Link
      to="/"
      className="font-display inline-flex items-baseline text-[21px] leading-none text-ink lowercase"
    >
      deku
      <span
        aria-hidden="true"
        className="ml-[2px] inline-block size-[6px] rounded-full bg-accent"
      />
    </Link>
  );
}

function NavItem({
  to,
  icon,
  label,
  shortcut,
  badge,
}: {
  to: '/' | '/services' | '/host' | '/settings';
  icon: ReactNode;
  label: string;
  shortcut: string;
  badge?: ReactNode;
}) {
  return (
    <Link
      to={to}
      activeOptions={{ exact: to === '/', includeSearch: false }}
      className="rounded-ui group flex h-8 items-center gap-2.5 px-2 text-[13px] text-ink-2 hover:bg-sunk hover:text-ink data-[status=active]:bg-sunk data-[status=active]:font-medium data-[status=active]:text-ink"
    >
      <span className="text-ink-3 group-data-[status=active]:text-ink">{icon}</span>
      <span className="flex-1">{label}</span>
      {badge}
      <span className="hidden group-hover:inline-flex">
        <Kbd>{shortcut}</Kbd>
      </span>
    </Link>
  );
}

function Sidebar({ onClose }: { onClose: () => void }) {
  const apps = useQuery(q.apps());
  const checks = useQuery(q.checks());
  const host = useQuery(q.host());
  const live = useSyncExternalStore(client.watchLive, client.isLive);
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const attention = (checks.data ?? []).filter((check) => check.status !== 'pass').length;
  const failing = (checks.data ?? []).some((check) => check.status === 'fail');

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-12 items-center justify-between border-b border-line px-4">
        <Wordmark />
        <button
          type="button"
          aria-label="Close menu"
          className="text-ink-3 lg:hidden"
          onClick={onClose}
        >
          <X className="size-4" />
        </button>
      </div>

      <nav aria-label="Primary" className="flex flex-col gap-px p-2">
        <NavItem to="/" icon={<Boxes className="size-4" />} label="Apps" shortcut="G A" />
        <NavItem
          to="/services"
          icon={<Database className="size-4" />}
          label="Services"
          shortcut="G S"
        />
        <NavItem
          to="/host"
          icon={<Server className="size-4" />}
          label="Host"
          shortcut="G H"
          badge={
            attention > 0 ? (
              <span className={`font-mono text-[11px] ${failing ? 'text-bad' : 'text-warn'}`}>
                {attention}
              </span>
            ) : null
          }
        />
        <NavItem
          to="/settings"
          icon={<Settings className="size-4" />}
          label="Settings"
          shortcut="G ,"
        />
      </nav>

      <div className="mt-2 min-h-0 flex-1 overflow-y-auto px-2">
        <p className="px-2 pb-1 font-mono text-[10.5px] tracking-[0.08em] text-ink-3 uppercase">
          Apps
        </p>
        <ul className="flex flex-col gap-px">
          {(apps.data ?? []).map((entry) => {
            const active = pathname.startsWith(`/apps/${entry.app.name}`);
            return (
              <li key={entry.app.id}>
                <Link
                  to="/apps/$app"
                  params={{ app: entry.app.name }}
                  className={`rounded-ui flex h-7 items-center gap-2.5 px-2 text-[13px] hover:bg-sunk ${
                    active ? 'bg-sunk text-ink' : 'text-ink-2'
                  }`}
                >
                  <Dot
                    tone={
                      entry.active
                        ? 'busy'
                        : entry.app.status === 'deployed'
                          ? entry.recent[0] === 'failed'
                            ? 'warn'
                            : 'ok'
                          : entry.app.status === 'error'
                            ? 'bad'
                            : 'idle'
                    }
                    pulse={Boolean(entry.active)}
                  />
                  <span className="truncate font-mono text-[12.5px]">{entry.app.name}</span>
                </Link>
              </li>
            );
          })}
        </ul>
      </div>

      <div className="border-t border-line px-4 py-3 text-[12px]">
        <div className="flex items-center gap-2">
          <Dot tone={live ? 'ok' : 'idle'} pulse={live} />
          <span className="text-ink">{host.data?.hostname ?? 'Connecting'}</span>
          <span className="ml-auto font-mono text-[11px] text-ink-3">{host.data?.version}</span>
        </div>
        <p className="mt-1 text-ink-3">{live ? 'Live updates on' : 'Reconnecting to the daemon'}</p>
      </div>
    </div>
  );
}

const THEME_NEXT: Record<ThemeMode, ThemeMode> = { system: 'light', light: 'dark', dark: 'system' };
const THEME_ICON: Record<ThemeMode, ReactNode> = {
  system: <Monitor className="size-4" />,
  light: <Sun className="size-4" />,
  dark: <Moon className="size-4" />,
};

function TopBar({ onMenu }: { onMenu: () => void }) {
  const prefs = usePrefs();
  return (
    <header className="sticky top-0 z-20 flex h-12 items-center gap-3 border-b border-line bg-bg/85 px-4 backdrop-blur sm:px-6 lg:px-8">
      <button
        type="button"
        aria-label="Open menu"
        className="text-ink-2 lg:hidden"
        onClick={onMenu}
      >
        <Menu className="size-5" />
      </button>
      <button
        type="button"
        onClick={openPalette}
        className="rounded-ui flex h-8 w-full max-w-[420px] items-center gap-2 border border-line bg-surface px-2.5 text-left text-[13px] text-ink-3 hover:border-line-2"
      >
        <Search className="size-4" />
        <span className="flex-1 truncate">Jump to an app, or run a command</span>
        <span className="hidden gap-0.5 sm:inline-flex">
          <Kbd>⌘</Kbd>
          <Kbd>K</Kbd>
        </span>
      </button>

      <div className="ml-auto flex items-center gap-2">
        <button
          type="button"
          onClick={() => setPrefs({ theme: THEME_NEXT[prefs.theme] })}
          aria-label={`Theme: ${prefs.theme}. Switch to ${THEME_NEXT[prefs.theme]}`}
          title={`Theme: ${prefs.theme}`}
          className="rounded-ui inline-flex size-8 items-center justify-center text-ink-2 hover:bg-sunk hover:text-ink"
        >
          {THEME_ICON[prefs.theme]}
        </button>
      </div>
    </header>
  );
}
