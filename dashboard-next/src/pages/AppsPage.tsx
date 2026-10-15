import { useMutation, useQuery } from '@tanstack/react-query';
import { Link, useNavigate } from '@tanstack/react-router';
import { ArrowRight, Plus, TriangleAlert } from 'lucide-react';
import { useState } from 'react';
import { DeployProgress } from '../components/DeployProgress';
import { Button, Cli, DeployStrip, Dot, Empty, toast } from '../components/ui';
import { deployDuration, relative, shortSha, useNow } from '../lib/format';
import { q } from '../lib/queries';
import type { AppSummary } from '../lib/types';
import { client } from '../mock/api';

export function AppsPage() {
  const apps = useQuery(q.apps());
  const checks = useQuery(q.checks());
  const now = useNow(1000);
  const [creating, setCreating] = useState(false);

  const list = apps.data ?? [];
  const failedLatest = list.filter((entry) => !entry.active && entry.recent[0] === 'failed');
  const hostIssues = (checks.data ?? []).filter((check) => check.status !== 'pass');
  const running = list.filter((entry) => entry.app.status === 'deployed').length;

  return (
    <div className="mx-auto max-w-[1280px]">
      <header className="mb-5 flex flex-wrap items-end gap-x-4 gap-y-3">
        <div>
          <h1 className="font-display text-[26px] leading-tight">Apps</h1>
          <p className="mt-0.5 text-ink-2">
            {list.length} apps, {running} running
          </p>
        </div>
        <div className="ml-auto flex items-center gap-2">
          <Button
            variant="primary"
            onClick={() => setCreating((value) => !value)}
            aria-expanded={creating}
          >
            <Plus className="size-4" />
            New app
          </Button>
        </div>
      </header>

      {creating ? <CreateApp onDone={() => setCreating(false)} /> : null}

      {failedLatest.length > 0 || hostIssues.length > 0 ? (
        <section
          aria-label="Needs attention"
          className="mb-6 flex flex-col gap-px overflow-hidden rounded-ui border border-line"
        >
          {failedLatest.map((entry) => (
            <Link
              key={entry.app.id}
              to="/apps/$app/deploys"
              params={{ app: entry.app.name }}
              className="group flex items-center gap-3 bg-surface px-3.5 py-2.5 hover:bg-sunk"
            >
              <Dot tone="bad" />
              <span className="min-w-0 flex-1 truncate">
                <span className="font-mono font-medium">{entry.app.name}</span>
                <span className="text-ink-2">
                  : the last deploy failed. The previous release{' '}
                  {entry.current ? (
                    <span className="font-mono">{shortSha(entry.current.id)}</span>
                  ) : null}{' '}
                  is still serving.
                </span>
              </span>
              <span className="text-[12.5px] text-ink-3 group-hover:text-ink">View deploy</span>
              <ArrowRight className="size-3.5 text-ink-3" />
            </Link>
          ))}
          {hostIssues.length > 0 ? (
            <Link
              to="/host"
              className="group flex items-center gap-3 bg-surface px-3.5 py-2.5 hover:bg-sunk"
            >
              <TriangleAlert className="size-3.5 text-warn" />
              <span className="min-w-0 flex-1 truncate text-ink-2">
                <span className="font-medium text-ink">
                  {hostIssues.length} host {hostIssues.length === 1 ? 'check needs' : 'checks need'}{' '}
                  attention
                </span>
                {': '}
                {hostIssues.map((check) => check.name.toLowerCase()).join(', ')}
              </span>
              <span className="text-[12.5px] text-ink-3 group-hover:text-ink">Open host</span>
              <ArrowRight className="size-3.5 text-ink-3" />
            </Link>
          ) : null}
        </section>
      ) : null}

      {apps.isPending ? (
        <ListSkeleton />
      ) : list.length === 0 ? (
        <Empty title="No apps yet" command="deku apps create <name>">
          Create an app, then deploy a source directory or an image to it.
        </Empty>
      ) : (
        <div className="text-[13px]">
          <div
            aria-hidden="true"
            className="hidden grid-cols-[minmax(180px,1.3fr)_minmax(150px,1fr)_minmax(130px,0.8fr)_110px_90px] gap-4 border-b border-line px-3 pb-2 font-mono text-[10.5px] tracking-[0.08em] text-ink-3 uppercase md:grid"
          >
            <span>App</span>
            <span>Release</span>
            <span>Processes</span>
            <span>Deploys</span>
            <span className="text-right">Updated</span>
          </div>
          {list.map((entry) => (
            <AppRow key={entry.app.id} entry={entry} now={now} />
          ))}
        </div>
      )}
    </div>
  );
}

function AppRow({ entry, now }: { entry: AppSummary; now: number }) {
  const { app, current, active } = entry;
  const latestFailed = !active && entry.recent[0] === 'failed';
  const tone = active
    ? 'busy'
    : app.status === 'deployed'
      ? latestFailed
        ? 'warn'
        : 'ok'
      : 'idle';
  const updated = active?.created_at ?? current?.created_at ?? app.created_at;

  return (
    <div className="relative border-b border-line hover:bg-sunk/60">
      <Link
        to="/apps/$app"
        params={{ app: app.name }}
        className="grid grid-cols-[1fr_auto] items-center gap-x-4 gap-y-1 px-3 py-3 md:grid-cols-[minmax(180px,1.3fr)_minmax(150px,1fr)_minmax(130px,0.8fr)_110px_90px]"
      >
        <span className="flex min-w-0 items-center gap-3">
          <Dot tone={tone} pulse={Boolean(active)} />
          <span className="min-w-0">
            <span className="block truncate font-mono text-[13.5px] font-medium text-ink">
              {app.name}
            </span>
            <span className="block truncate text-[12.5px] text-ink-3">
              {entry.domain ?? (app.status === 'created' ? 'Not deployed yet' : 'No domain')}
            </span>
          </span>
        </span>

        <span className="col-span-2 min-w-0 md:col-span-1">
          {active ? (
            <DeployProgress deployment={active} now={now} compact />
          ) : current ? (
            <span className="block min-w-0">
              <span className="font-mono text-[12.5px] text-ink">{shortSha(current.id)}</span>
              <span className="text-ink-3"> · {current.builder}</span>
              {latestFailed ? (
                <span className="block text-[12.5px] text-bad">Latest deploy failed</span>
              ) : (
                <span className="block text-[12.5px] text-ink-3">
                  took {deployDuration(current)}
                </span>
              )}
            </span>
          ) : (
            <span className="text-ink-3">No releases</span>
          )}
        </span>

        <span className="hidden min-w-0 font-mono text-[12px] text-ink-2 md:block">
          {entry.processes.length === 0
            ? '-'
            : entry.processes.map((process) => (
                <span key={process.process_type} className="block truncate">
                  {process.process_type}{' '}
                  <span className={process.healthy < process.scale ? 'text-warn' : 'text-ink-3'}>
                    {process.healthy}/{process.scale}
                  </span>
                </span>
              ))}
        </span>

        <span className="hidden md:block">
          {entry.recent.length > 0 ? (
            <DeployStrip statuses={entry.recent} />
          ) : (
            <span className="text-ink-3">-</span>
          )}
        </span>

        <span className="col-start-2 row-start-1 text-right text-[12.5px] whitespace-nowrap text-ink-3 md:col-auto md:row-auto">
          {relative(updated, now)}
        </span>
      </Link>
    </div>
  );
}

function CreateApp({ onDone }: { onDone: () => void }) {
  const [name, setName] = useState('');
  const navigate = useNavigate();
  const create = useMutation({
    mutationFn: (name: string) => client.createApp(name),
    onSuccess: (detail) => {
      toast(`Created ${detail.app.name}`);
      onDone();
      void navigate({ to: '/apps/$app', params: { app: detail.app.name } });
    },
  });
  return (
    <form
      className="mb-6 flex flex-wrap items-start gap-3 rounded-ui border border-line bg-surface p-3.5"
      onSubmit={(event) => {
        event.preventDefault();
        create.mutate(name.trim());
      }}
    >
      <label className="flex min-w-[240px] flex-1 flex-col gap-1.5">
        <span className="text-[12.5px] font-medium">App name</span>
        <input
          // biome-ignore lint/a11y/noAutofocus: the form opens because the user asked to type a name
          autoFocus
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder="shop"
          spellCheck={false}
          className="rounded-ui h-8 border border-line-2 bg-bg px-2.5 font-mono text-[13px] outline-none focus:border-ink-3"
        />
        {create.error ? (
          <span className="text-[12.5px] text-bad">{create.error.message}</span>
        ) : (
          <span className="text-[12.5px] text-ink-3">
            Lowercase letters, digits and dashes. It becomes the default subdomain.
          </span>
        )}
      </label>
      <div className="flex items-center gap-2 pt-[22px]">
        <Button type="submit" variant="primary" disabled={!name.trim() || create.isPending}>
          Create
        </Button>
        <Button variant="ghost" onClick={onDone}>
          Cancel
        </Button>
      </div>
      <div className="basis-full">
        <Cli command={`deku apps create ${name.trim() || '<name>'}`} />
      </div>
    </form>
  );
}

function ListSkeleton() {
  return (
    <div className="flex flex-col">
      {['a', 'b', 'c', 'd'].map((key) => (
        <div key={key} className="flex h-[58px] items-center gap-3 border-b border-line px-3">
          <span className="size-[7px] rounded-full bg-line-2" />
          <span className="h-3 w-40 rounded bg-line" />
        </div>
      ))}
    </div>
  );
}
