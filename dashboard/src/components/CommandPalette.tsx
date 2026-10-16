import { useMutation, useQuery } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { Command } from 'cmdk';
import {
  ArrowRight,
  BookOpen,
  Boxes,
  Database,
  Palette,
  Rocket,
  ScrollText,
  Server,
  Settings,
  Undo2,
} from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';
import { client } from '../api/client';
import { setPrefs } from '../lib/prefs';
import { q } from '../lib/queries';
import { toast } from './ui';

const OPEN_EVENT = 'deku:palette';

export function openPalette(): void {
  window.dispatchEvent(new Event(OPEN_EVENT));
}

function isTyping(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    Boolean(target.closest('input, textarea, select, [contenteditable="true"]'))
  );
}

const GO_TO: Record<string, '/' | '/services' | '/host' | '/settings'> = {
  a: '/',
  s: '/services',
  h: '/host',
  ',': '/settings',
};

export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const navigate = useNavigate();
  const apps = useQuery(q.apps());

  const deploy = useMutation({
    mutationFn: (app: string) => client.startDeploy(app),
    onSuccess: (deployment, app) => {
      toast(`Deploying ${app}`, 'busy');
      void navigate({
        to: '/apps/$app/deploys/$deployId',
        params: { app, deployId: deployment.id },
      });
    },
    onError: (error) => toast(error.message, 'bad'),
  });
  const rollback = useMutation({
    mutationFn: (app: string) => client.rollback(app),
    onSuccess: (deployment, app) => {
      toast(`Rolling back ${app}`, 'busy');
      void navigate({
        to: '/apps/$app/deploys/$deployId',
        params: { app, deployId: deployment.id },
      });
    },
    onError: (error) => toast(error.message, 'bad'),
  });

  useEffect(() => {
    const show = () => setOpen(true);
    let leader = 0;
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setOpen((value) => !value);
        return;
      }
      if (isTyping(event.target) || event.metaKey || event.ctrlKey || event.altKey) return;
      // "g" then a letter jumps between the top-level pages.
      if (event.key === 'g') {
        leader = window.setTimeout(() => {
          leader = 0;
        }, 900);
        return;
      }
      if (leader && GO_TO[event.key]) {
        window.clearTimeout(leader);
        leader = 0;
        void navigate({ to: GO_TO[event.key] });
      }
    };
    window.addEventListener(OPEN_EVENT, show);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener(OPEN_EVENT, show);
      window.removeEventListener('keydown', onKey);
    };
  }, [navigate]);

  const run = (action: () => void) => {
    setOpen(false);
    setSearch('');
    action();
  };

  const names = (apps.data ?? []).map((entry) => entry.app.name);
  // Only offer what would work: a redeploy needs an earlier source, a rollback an earlier release.
  const deployable = (apps.data ?? [])
    .filter((entry) => entry.current)
    .map((entry) => entry.app.name);
  const rollbackable = (apps.data ?? [])
    .filter((entry) => entry.recent.filter((status) => status === 'live').length > 1)
    .map((entry) => entry.app.name);

  return (
    <Command.Dialog
      open={open}
      onOpenChange={setOpen}
      label="Command palette"
      overlayClassName="fixed inset-0 z-50 bg-black/35"
      contentClassName="rounded-ui fixed top-[12vh] left-1/2 z-50 w-[min(640px,calc(100vw-32px))] -translate-x-1/2 overflow-hidden border border-line-2 bg-surface shadow-2xl shadow-black/25"
    >
      <Command.Input
        value={search}
        onValueChange={setSearch}
        placeholder="Jump to an app, or run a command"
        className="h-12 w-full border-b border-line bg-transparent px-4 text-[14px] text-ink outline-none placeholder:text-ink-3 focus-visible:outline-none"
      />
      <Command.List className="max-h-[min(440px,60vh)] overflow-y-auto pb-2">
        <Command.Empty className="px-4 py-6 text-ink-3">Nothing matches.</Command.Empty>

        <Command.Group heading="Go to">
          <Item icon={<Boxes />} onSelect={() => run(() => navigate({ to: '/' }))} hint="G A">
            Apps
          </Item>
          <Item
            icon={<Database />}
            onSelect={() => run(() => navigate({ to: '/services' }))}
            hint="G S"
          >
            Services
          </Item>
          <Item icon={<Server />} onSelect={() => run(() => navigate({ to: '/host' }))} hint="G H">
            Host
          </Item>
          <Item
            icon={<Settings />}
            onSelect={() => run(() => navigate({ to: '/settings' }))}
            hint="G ,"
          >
            Settings
          </Item>
          <Item
            icon={<BookOpen />}
            value="api reference docs openapi"
            onSelect={() => run(() => window.open('/api/docs', '_blank', 'noopener'))}
          >
            API reference
          </Item>
          {names.map((name) => (
            <Item
              key={`open-${name}`}
              icon={<ArrowRight />}
              value={`open ${name}`}
              onSelect={() => run(() => navigate({ to: '/apps/$app', params: { app: name } }))}
            >
              <span className="font-mono">{name}</span>
            </Item>
          ))}
        </Command.Group>

        <Command.Group heading="Actions">
          {deployable.map((name) => (
            <Item
              key={`deploy-${name}`}
              icon={<Rocket />}
              value={`redeploy deploy ${name}`}
              cli={`deku deploy run ${name} --path .`}
              onSelect={() => run(() => deploy.mutate(name))}
            >
              Redeploy <span className="font-mono">{name}</span>
            </Item>
          ))}
          {rollbackable.map((name) => (
            <Item
              key={`rollback-${name}`}
              icon={<Undo2 />}
              value={`roll back rollback ${name}`}
              cli={`deku deploy rollback ${name}`}
              onSelect={() => run(() => rollback.mutate(name))}
            >
              Roll back <span className="font-mono">{name}</span>
            </Item>
          ))}
          {names.map((name) => (
            <Item
              key={`logs-${name}`}
              icon={<ScrollText />}
              value={`logs tail ${name}`}
              cli={`deku logs ${name} --follow`}
              onSelect={() =>
                run(() => navigate({ to: '/apps/$app/logs', params: { app: name }, search: {} }))
              }
            >
              Tail logs for <span className="font-mono">{name}</span>
            </Item>
          ))}
        </Command.Group>

        <Command.Group heading="Appearance">
          <Item
            icon={<Palette />}
            value="theme light"
            onSelect={() => run(() => setPrefs({ theme: 'light' }))}
          >
            Light theme
          </Item>
          <Item
            icon={<Palette />}
            value="theme dark"
            onSelect={() => run(() => setPrefs({ theme: 'dark' }))}
          >
            Dark theme
          </Item>
          <Item
            icon={<Palette />}
            value="theme system"
            onSelect={() => run(() => setPrefs({ theme: 'system' }))}
          >
            Follow the system theme
          </Item>
        </Command.Group>
      </Command.List>
    </Command.Dialog>
  );
}

function Item({
  icon,
  children,
  onSelect,
  value,
  hint,
  cli,
}: {
  icon: ReactNode;
  children: ReactNode;
  onSelect: () => void;
  value?: string;
  hint?: string;
  cli?: string;
}) {
  return (
    <Command.Item
      value={value}
      onSelect={onSelect}
      className="mx-2 flex h-9 cursor-pointer items-center gap-3 rounded-[var(--radius)] px-2.5 text-[13.5px] text-ink [&>svg]:size-4 [&>svg]:shrink-0 [&>svg]:text-ink-3"
    >
      {icon}
      <span className="flex-1 truncate">{children}</span>
      {cli ? (
        <span className="hidden truncate font-mono text-[11px] text-ink-3 sm:inline">{cli}</span>
      ) : null}
      {hint ? <span className="font-mono text-[11px] text-ink-3">{hint}</span> : null}
    </Command.Item>
  );
}
