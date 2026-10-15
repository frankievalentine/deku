import { useQuery } from '@tanstack/react-query';
import { Link, useParams } from '@tanstack/react-router';
import { Lock, LockOpen } from 'lucide-react';
import { useState } from 'react';
import { LogView } from '../components/LogView';
import { Button, Cli, DeployState, Dot, Empty, Input, Section } from '../components/ui';
import { useAction, useDeployActions } from '../lib/actions';
import { absolute, deployDuration, relative, shortSha, useNow } from '../lib/format';
import { q } from '../lib/queries';
import type { ProcessRecord } from '../lib/types';
import { client } from '../mock/api';

const TRIGGER: Record<string, string> = {
  cli: 'from the CLI',
  git: 'from git push',
  token: 'by a deploy token',
  dashboard: 'from the dashboard',
};

export function AppOverview() {
  const { app } = useParams({ from: '/apps/$app' });
  const detail = useQuery(q.app(app));
  const deployments = useQuery(q.deployments(app));
  const logs = useQuery(q.logs(app));
  const services = useQuery(q.services());
  const now = useNow();
  const check = useAction(() => client.runHealthCheck(app));

  const list = deployments.data ?? [];
  const current = list.find((deployment) => deployment.status === 'live');

  if (deployments.isSuccess && list.length === 0) return <FirstDeploy app={app} />;

  return (
    <div className="grid gap-x-8 gap-y-8 xl:grid-cols-[minmax(0,1fr)_340px]">
      <div className="flex min-w-0 flex-col gap-8">
        <Section
          title="Current release"
          actions={
            current ? (
              <Link
                to="/apps/$app/deploys/$deployId"
                params={{ app, deployId: current.id }}
                className="text-[12.5px] text-ink-3 hover:text-ink"
              >
                Deploy details
              </Link>
            ) : null
          }
        >
          {current ? (
            <dl className="grid grid-cols-2 gap-x-6 gap-y-3 pt-1 sm:grid-cols-4">
              <Fact label="Release">
                <span className="font-mono">{shortSha(current.id)}</span>
              </Fact>
              <Fact label="Built with">{current.builder}</Fact>
              <Fact label="Went live">
                <span title={absolute(current.finished_at ?? current.created_at)}>
                  {relative(current.finished_at ?? current.created_at, now)}
                </span>
              </Fact>
              <Fact label="Took">{deployDuration(current)}</Fact>
              <div className="col-span-full text-[12.5px] text-ink-3">
                Deployed {TRIGGER[current.trigger]} as{' '}
                <span className="font-mono text-ink-2">{current.image_tag}</span>
              </div>
            </dl>
          ) : (
            <p className="text-ink-2">No release is live.</p>
          )}
        </Section>

        <Section
          title="Processes"
          actions={
            <>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => check.mutate(undefined)}
                disabled={check.isPending}
              >
                {check.isPending ? 'Checking…' : 'Run health check'}
              </Button>
              <Link
                to="/apps/$app/settings"
                params={{ app }}
                hash="processes"
                className="text-[12.5px] text-ink-3 hover:text-ink"
              >
                Scale
              </Link>
            </>
          }
        >
          <ul className="flex flex-col">
            {(detail.data?.processes ?? []).map((process) => (
              <ProcessRow key={process.process_type} process={process} />
            ))}
          </ul>
          {check.data ? (
            <ul className="rounded-ui mt-2 flex flex-col gap-1 bg-sunk px-3 py-2 font-mono text-[12px]">
              {check.data.map((result) => (
                <li key={result.container} className="flex items-center gap-2">
                  <Dot tone={result.status === 'pass' ? 'ok' : 'bad'} />
                  <span className="w-14">{result.container}</span>
                  <span className="text-ink-2">{result.detail}</span>
                  <span className="ml-auto text-ink-3">{result.latency_ms}ms</span>
                </li>
              ))}
            </ul>
          ) : null}
        </Section>

        <RunCommand app={app} />

        <Section
          title="Recent output"
          meta="live"
          actions={
            <Link
              to="/apps/$app/logs"
              params={{ app }}
              search={{}}
              className="text-[12.5px] text-ink-3 hover:text-ink"
            >
              Open logs
            </Link>
          }
        >
          <LogView lines={(logs.data ?? []).slice(-60)} height="h-[260px]" />
        </Section>
      </div>

      <aside className="flex min-w-0 flex-col gap-8">
        <Section title="Domains" meta={detail.data?.domains.length ?? 0}>
          {detail.data && detail.data.domains.length > 0 ? (
            <ul className="flex flex-col gap-2">
              {detail.data.domains.map((domain) => (
                <li key={domain.hostname} className="flex items-center gap-2 text-[13px]">
                  {domain.tls === 'active' ? (
                    <Lock className="size-3.5 text-ok" aria-label="HTTPS" />
                  ) : (
                    <LockOpen className="size-3.5 text-warn" aria-label="HTTP only" />
                  )}
                  <span className="min-w-0 flex-1 truncate">{domain.hostname}</span>
                  <span
                    className={`text-[12px] ${domain.tls === 'active' ? 'text-ink-3' : 'text-warn'}`}
                  >
                    {domain.state === 'active' || domain.state === 'manual'
                      ? 'HTTPS'
                      : domain.state === 'issuing'
                        ? 'Requesting certificate'
                        : domain.state === 'checking'
                          ? 'Waiting for DNS'
                          : domain.state === 'failed'
                            ? 'Certificate paused'
                            : 'HTTP only'}
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <Cli command={`deku domains add ${app} <hostname>`} />
          )}
        </Section>

        <Section title="Linked services" meta={detail.data?.linked_services.length ?? 0}>
          {detail.data && detail.data.linked_services.length > 0 ? (
            <ul className="flex flex-col gap-2">
              {detail.data.linked_services.map((name) => {
                const service = services.data?.find((entry) => entry.name === name);
                return (
                  <li key={name} className="flex items-center gap-2 text-[13px]">
                    <Dot tone={service?.status === 'running' ? 'ok' : 'idle'} />
                    <Link to="/services" className="font-mono hover:underline">
                      {name}
                    </Link>
                    <span className="ml-auto text-[12px] text-ink-3">
                      {service ? `${service.kind} ${service.version}` : ''}
                    </span>
                  </li>
                );
              })}
            </ul>
          ) : (
            <Cli command={`deku postgres link <service> ${app}`} />
          )}
        </Section>

        <Section
          title="Recent deploys"
          actions={
            <Link
              to="/apps/$app/deploys"
              params={{ app }}
              className="text-[12.5px] text-ink-3 hover:text-ink"
            >
              All
            </Link>
          }
        >
          <ul className="flex flex-col">
            {list.slice(0, 5).map((deployment) => (
              <li key={deployment.id}>
                <Link
                  to="/apps/$app/deploys/$deployId"
                  params={{ app, deployId: deployment.id }}
                  className="-mx-2 flex items-center gap-3 rounded-ui px-2 py-1.5 text-[13px] hover:bg-sunk"
                >
                  <span className="w-[62px] font-mono text-[12px] text-ink-2">
                    {shortSha(deployment.id)}
                  </span>
                  <DeployState status={deployment.status} current={deployment.id === current?.id} />
                  <span className="ml-auto text-[12px] text-ink-3">
                    {relative(deployment.created_at, now)}
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        </Section>
      </aside>
    </div>
  );
}

function Fact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="min-w-0">
      <dt className="text-[12px] text-ink-3">{label}</dt>
      <dd className="mt-0.5 truncate text-[14px] text-ink">{children}</dd>
    </div>
  );
}

function ProcessRow({ process }: { process: ProcessRecord }) {
  const replicas = Array.from({ length: process.scale }, (_, index) => index + 1);
  return (
    <li className="flex flex-wrap items-center gap-x-6 gap-y-1 border-b border-line py-2.5 last:border-b-0">
      <span className="w-[110px] font-mono text-[13px] font-medium">{process.process_type}</span>
      <span
        className="flex items-center gap-1.5"
        role="img"
        aria-label={`${process.healthy} of ${process.scale} healthy`}
      >
        {replicas.map((replica) => (
          <span
            key={replica}
            title={`${process.process_type}.${replica}`}
            className={`size-3 rounded-[2px] ${replica <= process.healthy ? 'bg-ok' : 'bg-busy dot-pulse'}`}
          />
        ))}
        {process.scale === 0 ? <span className="text-ink-3">Scaled to zero</span> : null}
      </span>
      <span className="tabular font-mono text-[12px] text-ink-2">
        {process.healthy}/{process.scale} healthy
      </span>
      <span className="ml-auto font-mono text-[12px] text-ink-3">
        {[
          process.memory_limit ? `${process.memory_limit} mem` : 'no memory limit',
          process.cpu_limit ? `${process.cpu_limit} cpu` : null,
        ]
          .filter(Boolean)
          .join(' · ')}
      </span>
    </li>
  );
}

function FirstDeploy({ app }: { app: string }) {
  const [image, setImage] = useState('');
  const { deploy } = useDeployActions(app);
  return (
    <div className="grid gap-6 lg:grid-cols-2">
      <Section title="Deploy from a source directory">
        <p className="mb-3 max-w-prose text-ink-2">
          Deku builds with your Dockerfile, or with Railpack when there is none. Run this from the
          project directory:
        </p>
        <Cli command={`deku deploy run ${app} --path .`} />
        <p className="mt-4 mb-3 max-w-prose text-ink-2">
          Or add a git remote, then <span className="font-mono">git push deku main</span>:
        </p>
        <Cli command={`deku git remote add ${app} --host <server>`} />
      </Section>
      <Section title="Deploy an image">
        <form
          className="flex flex-col gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            if (image.trim()) deploy.mutate(image.trim());
          }}
        >
          <label className="flex flex-col gap-1.5">
            <span className="text-[12.5px] font-medium">Image reference</span>
            <input
              value={image}
              onChange={(event) => setImage(event.target.value)}
              placeholder="ghcr.io/acme/web:1.4.2"
              spellCheck={false}
              className="rounded-ui h-8 border border-line-2 bg-surface px-2.5 font-mono text-[13px] outline-none focus:border-ink-3"
            />
          </label>
          <div className="flex items-center gap-3">
            <Button type="submit" variant="primary" disabled={!image.trim() || deploy.isPending}>
              Deploy image
            </Button>
            <Cli command={`deku deploy run ${app} --image ${image.trim() || '<ref>'}`} />
          </div>
        </form>
      </Section>
      <div className="lg:col-span-2">
        <Empty title="Nothing is running yet">
          The first successful deploy gets {app}'s default domain and a certificate when a global
          domain is configured.
        </Empty>
      </div>
    </div>
  );
}

/** Run a one-off command: in a fresh container by default, or inside a running one to inspect it. */
function RunCommand({ app }: { app: string }) {
  const [command, setCommand] = useState('');
  const [mode, setMode] = useState<'fresh' | 'live'>('fresh');
  const [history, setHistory] = useState<
    Array<{ command: string; mode: string; output: string[]; exit: number }>
  >([]);
  const run = useAction(({ cmd, where }: { cmd: string; where: 'fresh' | 'live' }) =>
    client.runCommand(app, cmd, where)
  );

  return (
    <Section
      title="Run a command"
      actions={
        <Cli
          command={`deku ${mode === 'fresh' ? 'run' : 'exec'} ${app} ${command.trim() || '<command>'}`}
        />
      }
    >
      <form
        className="flex flex-wrap items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          const cmd = command.trim();
          if (!cmd) return;
          run.mutate(
            { cmd, where: mode },
            {
              onSuccess: (result) => {
                setHistory((items) =>
                  [
                    { command: cmd, mode, output: result.output, exit: result.exit_code },
                    ...items,
                  ].slice(0, 5)
                );
                setCommand('');
              },
            }
          );
        }}
      >
        <fieldset className="rounded-ui flex h-8 items-center border border-line-2 bg-surface p-0.5">
          <legend className="sr-only">Where to run it</legend>
          {(
            [
              ['fresh', 'Fresh container'],
              ['live', 'Running container'],
            ] as const
          ).map(([value, label]) => (
            <label
              key={value}
              className={`rounded-ui flex h-full cursor-pointer items-center px-2.5 text-[12.5px] ${
                mode === value ? 'bg-sunk text-ink' : 'text-ink-3 hover:text-ink'
              }`}
            >
              <input
                type="radio"
                name="run-mode"
                checked={mode === value}
                onChange={() => setMode(value)}
                className="sr-only"
              />
              {label}
            </label>
          ))}
        </fieldset>
        <Input
          mono
          aria-label="Command"
          value={command}
          onChange={(event) => setCommand(event.target.value)}
          placeholder="bun run db:seed"
          className="min-w-[220px] flex-1"
        />
        <Button type="submit" disabled={!command.trim() || run.isPending}>
          {run.isPending ? 'Running…' : 'Run'}
        </Button>
      </form>
      <p className="mt-2 text-[12.5px] text-ink-3">
        {mode === 'fresh'
          ? 'A fresh container gets the app config and storage, and is removed when the command exits.'
          : 'Changes made inside the running container are lost on the next deploy. Use it to inspect.'}
      </p>
      {history.length > 0 ? (
        <div className="rounded-ui mt-3 max-h-[260px] overflow-auto bg-term px-3.5 py-2.5 font-mono text-[12px] leading-[1.65] text-term-ink">
          {history.map((entry, index) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: a run history has no stable identity
            <div key={index} className="mb-2 last:mb-0">
              <div className="text-term-ink/55">
                $ {entry.command}{' '}
                <span className="text-term-ink/35">
                  ({entry.mode === 'fresh' ? 'fresh' : 'running'}, exit {entry.exit})
                </span>
              </div>
              {entry.output.map((line, lineIndex) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: output lines are positional
                <div key={lineIndex}>{line}</div>
              ))}
            </div>
          ))}
        </div>
      ) : null}
    </Section>
  );
}
