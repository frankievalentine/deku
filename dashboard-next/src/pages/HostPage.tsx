import { useQuery } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { Check, Lock, LockOpen } from 'lucide-react';
import { useState } from 'react';
import {
  Button,
  CHECK_TONE,
  Cli,
  ConfirmButton,
  Dot,
  Field,
  Input,
  Meter,
  Row,
  Section,
} from '../components/ui';
import { useAction } from '../lib/actions';
import { absolute, relative, uptime, useNow } from '../lib/format';
import { q } from '../lib/queries';
import type { DoctorCheck } from '../lib/types';
import { client } from '../mock/api';

const SEVERITY: Record<DoctorCheck['status'], number> = { fail: 0, warn: 1, pass: 2 };

export function HostPage() {
  const checks = useQuery(q.checks());
  const host = useQuery(q.host());
  const events = useQuery(q.events());
  const now = useNow();
  const [showPassing, setShowPassing] = useState(false);

  const sorted = (checks.data ?? [])
    .slice()
    .sort((a, b) => SEVERITY[a.status] - SEVERITY[b.status]);
  const issues = sorted.filter((check) => check.status !== 'pass');
  const passing = sorted.filter((check) => check.status === 'pass');
  const h = host.data;

  return (
    <div className="mx-auto max-w-[1280px]">
      <header className="mb-6 flex flex-wrap items-end gap-x-6 gap-y-3">
        <div>
          <h1 className="font-display text-[26px] leading-tight">{h?.hostname ?? 'Host'}</h1>
          <p className="mt-0.5 text-ink-2">
            {h ? (
              <>
                Deku {h.version} · up {uptime(h.uptime_seconds)}
                {h.latest_version !== h.version ? (
                  <span className="text-accent-ink"> · {h.latest_version} is available</span>
                ) : null}
              </>
            ) : (
              'Loading'
            )}
          </p>
        </div>
        <Cli className="ml-auto" command="deku doctor" />
      </header>

      <div className="grid gap-x-10 gap-y-10 xl:grid-cols-[minmax(0,1fr)_360px]">
        <div className="flex min-w-0 flex-col gap-10">
          <Section
            title="Checks"
            meta={
              issues.length === 0
                ? 'all passing'
                : `${issues.length} need attention · ${passing.length} passing`
            }
          >
            {issues.length === 0 ? (
              <p className="flex items-center gap-2 py-2 text-ink-2">
                <Check className="size-4 text-ok" /> Every check passes.
              </p>
            ) : (
              <ul className="flex flex-col gap-3 pt-1">
                {issues.map((check) => (
                  <li key={check.id} className="rounded-ui border border-line bg-surface p-3.5">
                    <div className="flex items-center gap-2.5">
                      <Dot tone={CHECK_TONE[check.status]} />
                      <h3 className="font-medium">{check.name}</h3>
                      <span
                        className={`ml-auto font-mono text-[11px] uppercase ${check.status === 'fail' ? 'text-bad' : 'text-warn'}`}
                      >
                        {check.status === 'fail' ? 'failing' : 'warning'}
                      </span>
                    </div>
                    <p className="mt-1.5 pl-[17px] text-ink-2">{check.detail}</p>
                    {check.fix ? <p className="mt-1 pl-[17px] text-ink">{check.fix}</p> : null}
                    {check.command ? (
                      <Cli className="mt-2.5 ml-[17px]" command={check.command} />
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
            <button
              type="button"
              onClick={() => setShowPassing((value) => !value)}
              aria-expanded={showPassing}
              className="mt-3 text-[12.5px] text-ink-3 hover:text-ink"
            >
              {showPassing ? 'Hide' : 'Show'} {passing.length} passing checks
            </button>
            {showPassing ? (
              <ul className="mt-2 border-t border-line">
                {passing.map((check) => (
                  <li
                    key={check.id}
                    className="flex items-center gap-2.5 border-b border-line py-2 text-[13px]"
                  >
                    <Dot tone="ok" />
                    <span className="w-[180px] shrink-0">{check.name}</span>
                    <span className="min-w-0 truncate text-ink-3">{check.detail}</span>
                  </li>
                ))}
              </ul>
            ) : null}
          </Section>

          <Routing />

          <Section title="Activity">
            <ul className="flex flex-col">
              {(events.data ?? []).map((event) => (
                <li
                  key={event.id}
                  className="flex items-baseline gap-3 border-b border-line py-2 text-[13px] last:border-b-0"
                >
                  <span className="w-[68px] shrink-0 text-[12px] text-ink-3">
                    {relative(event.created_at, now)}
                  </span>
                  <span className="w-[170px] shrink-0 truncate font-mono text-[12px] text-ink-2">
                    {event.kind}
                  </span>
                  <span className="min-w-0 flex-1">
                    {event.app ? (
                      <Link
                        to="/apps/$app"
                        params={{ app: event.app }}
                        className="font-mono font-medium hover:underline"
                      >
                        {event.app}
                      </Link>
                    ) : null}
                    {event.app ? ' ' : null}
                    <span className="text-ink-2">{event.message}</span>
                  </span>
                </li>
              ))}
            </ul>
          </Section>
        </div>

        <aside className="flex min-w-0 flex-col gap-10">
          <Section title="Resources">
            {h ? (
              <dl className="flex flex-col gap-4 pt-1">
                <Resource
                  label="Disk"
                  value={`${h.disk.used_gb} / ${h.disk.total_gb} GB`}
                  ratio={h.disk.used_gb / h.disk.total_gb}
                />
                <Resource
                  label="Memory"
                  value={`${(h.memory.used_mb / 1024).toFixed(1)} / ${(h.memory.total_mb / 1024).toFixed(1)} GB`}
                  ratio={h.memory.used_mb / h.memory.total_mb}
                />
                <Resource label="CPU" value={`${h.cpu_percent}%`} ratio={h.cpu_percent / 100} />
                <div className="flex items-baseline justify-between text-[13px]">
                  <dt className="text-ink-2">Load</dt>
                  <dd className="tabular font-mono text-[12.5px]">
                    {h.load.map((value) => value.toFixed(2)).join('  ')}
                  </dd>
                </div>
              </dl>
            ) : null}
          </Section>

          <ProxyStatusSection />
          <Networks />

          <Section title="Access">
            <dl className="flex flex-col gap-2 text-[13px]">
              <div className="flex justify-between gap-4">
                <dt className="text-ink-2">Dashboard</dt>
                <dd className="font-mono text-[12.5px]">127.0.0.1:2810</dd>
              </div>
              <div className="flex justify-between gap-4">
                <dt className="text-ink-2">Git over SSH</dt>
                <dd className="font-mono text-[12.5px]">:2222</dd>
              </div>
              <div className="flex justify-between gap-4">
                <dt className="text-ink-2">Proxy</dt>
                <dd className="font-mono text-[12.5px]">Angie on :80, :443</dd>
              </div>
            </dl>
          </Section>
        </aside>
      </div>
    </div>
  );
}

function Resource({ label, value, ratio }: { label: string; value: string; ratio: number }) {
  const tone = ratio > 0.9 ? 'bad' : ratio > 0.8 ? 'warn' : 'idle';
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-baseline justify-between text-[13px]">
        <dt className="text-ink-2">{label}</dt>
        <dd
          className={`tabular font-mono text-[12.5px] ${tone === 'warn' ? 'text-warn' : tone === 'bad' ? 'text-bad' : ''}`}
        >
          {value}
        </dd>
      </div>
      <Meter value={ratio} max={1} tone={tone} />
    </div>
  );
}

function Routing() {
  const routes = useQuery(q.routes());
  const list = routes.data ?? [];
  return (
    <Section
      title="Routing"
      meta={`${list.length} hostnames`}
      actions={<Cli command="deku checks routing" />}
    >
      <div className="overflow-x-auto">
        <table className="w-full min-w-[560px] text-left text-[13px]">
          <thead>
            <tr className="font-mono text-[10.5px] tracking-[0.08em] text-ink-3 uppercase">
              <th className="py-2 pr-4 font-normal">Hostname</th>
              <th className="py-2 pr-4 font-normal">App</th>
              <th className="py-2 pr-4 font-normal">Upstreams</th>
            </tr>
          </thead>
          <tbody>
            {list.map((route) => (
              <tr key={route.hostname} className="border-t border-line align-top">
                <td className="py-2 pr-4">
                  <span className="flex items-center gap-2">
                    {route.tls === 'active' ? (
                      <Lock className="size-3.5 shrink-0 text-ok" aria-label="HTTPS" />
                    ) : (
                      <LockOpen className="size-3.5 shrink-0 text-ink-3" aria-label="HTTP only" />
                    )}
                    <span className="truncate">{route.hostname}</span>
                  </span>
                </td>
                <td className="py-2 pr-4">
                  <Link
                    to="/apps/$app"
                    params={{ app: route.app }}
                    className="font-mono hover:underline"
                  >
                    {route.app}
                  </Link>
                  {route.environment !== 'production' ? (
                    <span className="ml-1.5 text-[12px] text-ink-3">{route.environment}</span>
                  ) : null}
                </td>
                <td className="py-2 pr-4 font-mono text-[12px] text-ink-2">
                  {route.upstreams.length === 0 ? (
                    <span className="text-warn">none running</span>
                  ) : (
                    route.upstreams.join(', ')
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Section>
  );
}

function ProxyStatusSection() {
  const proxy = useQuery(q.proxy());
  const now = useNow();
  const reload = useAction(() => client.reloadProxy(), 'Proxy configuration checked and reloaded');
  const p = proxy.data;
  return (
    <Section title="Proxy">
      {p ? (
        <div className="flex flex-col gap-3 text-[13px]">
          <span className="inline-flex items-center gap-2">
            <Dot tone={p.valid ? 'ok' : 'bad'} />
            {p.valid ? 'Angie configuration is valid' : 'Angie configuration has errors'}
          </span>
          <dl className="flex flex-col gap-1.5">
            <div className="flex justify-between gap-4">
              <dt className="text-ink-2">Last reload</dt>
              <dd title={absolute(p.reloaded_at)}>{relative(p.reloaded_at, now)}</dd>
            </div>
            <div className="flex justify-between gap-4">
              <dt className="text-ink-2">Files</dt>
              <dd className="font-mono text-[12.5px]">
                {p.files} in {p.config_path}
              </dd>
            </div>
          </dl>
          <Button
            size="sm"
            className="self-start"
            onClick={() => reload.mutate(undefined)}
            disabled={reload.isPending}
          >
            Check and reload
          </Button>
        </div>
      ) : null}
    </Section>
  );
}

function Networks() {
  const networks = useQuery(q.networks());
  const [name, setName] = useState('');
  const create = useAction((value: string) => client.createNetwork(value), 'Network created');
  const remove = useAction((value: string) => client.removeNetwork(value), 'Network removed');
  return (
    <Section title="Docker networks" meta={networks.data?.length ?? 0}>
      <ul>
        {(networks.data ?? []).map((network) => (
          <Row
            key={network.name}
            actions={
              network.managed ? null : (
                <ConfirmButton
                  label="Remove"
                  confirmLabel="Detach every app and remove"
                  onConfirm={() => remove.mutate(network.name)}
                />
              )
            }
          >
            <span className="block font-mono text-[12.5px]">{network.name}</span>
            <span className="block truncate text-[12px] text-ink-3">
              {network.managed ? 'Managed by Deku · ' : ''}
              {network.apps.length === 0 ? 'no apps' : network.apps.join(', ')}
            </span>
          </Row>
        ))}
      </ul>
      <form
        className="mt-3 flex items-start gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          create.mutate(name.trim(), { onSuccess: () => setName('') });
        }}
      >
        <Field label="New network" className="min-w-0 flex-1">
          <Input
            mono
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="payments-net"
          />
        </Field>
        <Button type="submit" className="mt-6" disabled={!name.trim() || create.isPending}>
          Create
        </Button>
      </form>
    </Section>
  );
}
