import { useQuery } from '@tanstack/react-query';
import { useParams } from '@tanstack/react-router';
import { Check, Copy, Lock, LockOpen, Minus, Plus } from 'lucide-react';
import { useState } from 'react';
import { client } from '../api/client';
import {
  Button,
  Cli,
  ConfirmButton,
  Dot,
  Field,
  Input,
  Row,
  Section,
  SectionNav,
  Select,
  type Tone,
  toast,
} from '../components/ui';
import { useAction } from '../lib/actions';
import { relative, useNow } from '../lib/format';
import { q } from '../lib/queries';
import type { AppDetail, AuthConfig, Domain, ProcessRecord, Redirect } from '../lib/types';

const SECTIONS: Array<[string, string]> = [
  ['domains', 'Domains and HTTPS'],
  ['ports', 'Ports'],
  ['access', 'Access'],
  ['redirects', 'Redirects'],
  ['maintenance', 'Maintenance'],
  ['processes', 'Processes'],
  ['environments', 'Environments'],
  ['tokens', 'Deploy tokens'],
  ['storage', 'Storage'],
  ['networks', 'Networks'],
  ['cron', 'Scheduled jobs'],
  ['danger', 'Danger zone'],
];

export function AppSettings() {
  const { app } = useParams({ from: '/apps/$app' });
  const detail = useQuery(q.app(app));
  const data = detail.data;
  if (!data) return <div className="h-40" />;

  return (
    <div className="grid gap-10 xl:grid-cols-[minmax(0,1fr)_180px]">
      <div className="flex max-w-[880px] min-w-0 flex-col gap-12">
        <Domains app={app} data={data} />
        <Ports app={app} data={data} />
        <Access app={app} data={data} />
        <Redirects app={app} data={data} />
        <Maintenance app={app} data={data} />
        <Processes app={app} data={data} />
        <Environments app={app} data={data} />
        <DeployTokens app={app} data={data} />
        <Storage app={app} data={data} />
        <Networks app={app} data={data} />
        <ScheduledJobs app={app} data={data} />
        <DangerZone app={app} />
      </div>
      <SectionNav items={SECTIONS} />
    </div>
  );
}

interface Props {
  app: string;
  data: AppDetail;
}

/** How a domain is served right now, and why it is waiting if it is. */
function domainStatus(
  domain: Domain,
  installed: AppDetail['certificate'],
  now: number
): { tone: Tone; text: string } {
  switch (domain.state) {
    case 'active': {
      const days = domain.expires_at
        ? Math.round((new Date(domain.expires_at).getTime() - now) / 86_400_000)
        : null;
      return {
        tone: 'ok',
        text:
          days === null
            ? 'HTTPS, renews automatically'
            : `HTTPS, renews automatically (${days} days left)`,
      };
    }
    case 'manual': {
      // Deku serves an installed certificate but never renews it, so its expiry
      // is the thing to watch.
      if (!installed.present) {
        return { tone: 'bad', text: 'Set to an installed certificate, but none is installed' };
      }
      if (!installed.expires_at) {
        return { tone: 'ok', text: 'HTTPS with your installed certificate' };
      }
      const days = Math.floor((new Date(installed.expires_at).getTime() - now) / 86_400_000);
      if (days < 0) return { tone: 'bad', text: 'Your installed certificate has expired' };
      return {
        tone: days < 14 ? 'warn' : 'ok',
        text: `HTTPS with your installed certificate (${days} ${days === 1 ? 'day' : 'days'} left, renew it yourself)`,
      };
    }
    case 'issuing':
      return { tone: 'busy', text: 'Requesting a certificate' };
    case 'checking':
      return { tone: 'warn', text: domain.details ?? 'Checking that it reaches this server' };
    case 'failed':
      return { tone: 'bad', text: `Requests paused. ${domain.details ?? ''}`.trim() };
    case 'off':
      return { tone: 'idle', text: 'HTTP only' };
    default:
      return { tone: 'idle', text: 'HTTP. Automatic certificates are off for this host.' };
  }
}

function Domains({ app, data }: Props) {
  const now = useNow();
  const [hostname, setHostname] = useState('');
  const add = useAction(
    (host: string) => client.addDomain(app, host),
    'Domain added. Checking that it reaches this server.'
  );
  const remove = useAction((host: string) => client.removeDomain(app, host), 'Domain removed');
  const mode = useAction(
    ({ host, value }: { host: string; value: Domain['mode'] }) =>
      client.setDomainMode(app, host, value),
    (_, { host, value }) =>
      `${host}: ${value === 'auto' ? 'automatic certificate' : value === 'manual' ? 'installed certificate' : 'HTTP only'}`
  );
  const retry = useAction((host: string) => client.retryDomain(app, host), 'Checking again');
  const manualInUse = data.domains.some((domain) => domain.mode === 'manual');

  return (
    <Section
      id="domains"
      title="Domains and HTTPS"
      actions={<Cli command={`deku domains add ${app} <hostname>`} />}
    >
      <p className="max-w-prose text-ink-2">
        Point a domain's DNS at this server and Deku requests its certificate and keeps it renewed.
      </p>
      <ul className="mt-3">
        {data.domains.map((domain) => {
          const status = domainStatus(domain, data.certificate, now);
          return (
            <Row
              key={domain.hostname}
              actions={
                <>
                  {domain.state === 'failed' || domain.state === 'checking' ? (
                    <Button
                      size="sm"
                      onClick={() => retry.mutate(domain.hostname)}
                      disabled={retry.isPending}
                    >
                      Retry now
                    </Button>
                  ) : null}
                  <Select
                    aria-label={`How ${domain.hostname} gets its certificate`}
                    value={domain.mode}
                    onChange={(event) =>
                      mode.mutate({
                        host: domain.hostname,
                        value: event.target.value as Domain['mode'],
                      })
                    }
                    className="h-7 text-[12.5px]"
                  >
                    <option value="auto">Automatic</option>
                    <option value="manual">Installed</option>
                    <option value="off">HTTP only</option>
                  </Select>
                  <ConfirmButton
                    label="Remove"
                    confirmLabel="Stop serving this domain"
                    onConfirm={() => remove.mutate(domain.hostname)}
                  />
                </>
              }
            >
              <span className="flex items-center gap-2">
                {domain.tls === 'active' ? (
                  <Lock className="size-3.5 shrink-0 text-ok" aria-label="HTTPS" />
                ) : (
                  <LockOpen className="size-3.5 shrink-0 text-ink-3" aria-label="HTTP" />
                )}
                <span className="truncate font-medium">{domain.hostname}</span>
              </span>
              <span className="mt-0.5 flex items-start gap-1.5 pl-[22px] text-[12.5px] text-ink-2">
                <Dot tone={status.tone} pulse={domain.state === 'issuing'} className="mt-[5px]" />
                <span>{status.text}</span>
              </span>
            </Row>
          );
        })}
        {data.domains.length === 0 ? (
          <li className="py-2 text-ink-2">
            No domains yet. Add the hostname your users will visit.
          </li>
        ) : null}
      </ul>
      <form
        className="mt-3 flex flex-wrap items-start gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          add.mutate(hostname, { onSuccess: () => setHostname('') });
        }}
      >
        <Field
          label="Hostname"
          className="min-w-[240px] flex-1"
          hint="Point its DNS record at this server first."
        >
          <Input
            mono
            value={hostname}
            onChange={(e) => setHostname(e.target.value)}
            placeholder="shop.example.com"
          />
        </Field>
        <Button type="submit" className="mt-6" disabled={!hostname.trim() || add.isPending}>
          Add domain
        </Button>
      </form>
      {manualInUse ? (
        <p className="rounded-ui mt-4 border border-line p-3.5 text-[13px] text-ink-2">
          Installed certificates are read from{' '}
          <span className="font-mono text-ink">
            {data.certificate.path.replace(/\.crt$/, '.{crt,key}')}
          </span>
          . Deku serves them but does not renew them.
        </p>
      ) : null}
      <Cli className="mt-3" command={`deku domains tls ${app} <domain> auto|manual|off`} />
    </Section>
  );
}

function Ports({ app, data }: Props) {
  const [hostPort, setHostPort] = useState('80');
  const [containerPort, setContainerPort] = useState('3000');
  const add = useAction(
    () => client.addPort(app, Number(hostPort), Number(containerPort)),
    'Port mapping added'
  );
  const remove = useAction((id: string) => client.removePort(app, id), 'Port mapping removed');
  const valid = Number(hostPort) > 0 && Number(containerPort) > 0;

  return (
    <Section id="ports" title="Ports">
      <p className="max-w-prose text-ink-2">
        Which port the app listens on inside its container, for each public port the proxy serves.
      </p>
      <ul className="mt-3">
        {data.ports.map((port) => (
          <Row
            key={port.id}
            actions={
              <ConfirmButton
                label="Remove"
                confirmLabel="Remove mapping"
                onConfirm={() => remove.mutate(port.id)}
              />
            }
          >
            <span className="font-mono text-[12.5px]">
              {port.host_port} <span className="text-ink-3">to container port</span>{' '}
              {port.container_port}
            </span>
          </Row>
        ))}
        {data.ports.length === 0 ? (
          <li className="py-2 text-ink-2">
            No mappings. The first deploy adds one from the port in deku.toml or the image.
          </li>
        ) : null}
      </ul>
      <form
        className="mt-3 flex flex-wrap items-start gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (valid) add.mutate(undefined);
        }}
      >
        <Field label="Public port">
          <Input
            mono
            inputMode="numeric"
            className="w-24"
            value={hostPort}
            onChange={(e) => setHostPort(e.target.value)}
          />
        </Field>
        <Field label="Container port">
          <Input
            mono
            inputMode="numeric"
            className="w-28"
            value={containerPort}
            onChange={(e) => setContainerPort(e.target.value)}
          />
        </Field>
        <Button type="submit" className="mt-6" disabled={!valid || add.isPending}>
          Add mapping
        </Button>
      </form>
    </Section>
  );
}

function Access({ app, data }: Props) {
  const [mode, setMode] = useState<AuthConfig['mode']>(data.auth.mode);
  const [username, setUsername] = useState(data.auth.mode === 'basic' ? data.auth.username : '');
  const [password, setPassword] = useState('');
  const [url, setUrl] = useState(data.auth.mode === 'forward' ? data.auth.url : '');
  const save = useAction((auth: AuthConfig) => client.setAuth(app, auth), 'Access saved');

  const next: AuthConfig =
    mode === 'basic'
      ? { mode, username, password }
      : mode === 'forward'
        ? { mode, url }
        : { mode: 'none' };
  const cli =
    mode === 'basic'
      ? `deku auth enable ${app} --user ${username || '<user>'}`
      : mode === 'forward'
        ? `deku auth forward ${app} --url ${url || '<url>'}`
        : `deku auth disable ${app}`;

  return (
    <Section id="access" title="Access">
      <p className="max-w-prose text-ink-2">
        Who can reach the app. The proxy checks every request.
      </p>
      <form
        className="mt-3 flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          save.mutate(next, { onSuccess: () => setPassword('') });
        }}
      >
        <fieldset className="flex flex-wrap gap-2">
          <legend className="sr-only">Access mode</legend>
          {(
            [
              ['none', 'Anyone'],
              ['basic', 'Username and password'],
              ['forward', 'Another service decides'],
            ] as const
          ).map(([value, label]) => (
            <label
              key={value}
              className={`rounded-ui cursor-pointer border px-3 py-1.5 text-[13px] ${
                mode === value
                  ? 'border-ink-3 bg-sunk text-ink'
                  : 'border-line text-ink-2 hover:text-ink'
              }`}
            >
              <input
                type="radio"
                name="access"
                value={value}
                checked={mode === value}
                onChange={() => setMode(value)}
                className="sr-only"
              />
              {label}
            </label>
          ))}
        </fieldset>
        {mode === 'basic' ? (
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="Username">
              <Input
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                autoComplete="off"
              />
            </Field>
            <Field label="Password" hint="Saving sets this password; the old one stops working.">
              <Input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="new-password"
              />
            </Field>
          </div>
        ) : null}
        {mode === 'forward' ? (
          <Field
            label="Auth service URL"
            hint="Each request is sent here first. A 2xx response lets it through; anything else is refused."
          >
            <Input
              mono
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="http://deku-auth-proxy:4180/oauth2/auth"
            />
          </Field>
        ) : null}
        <div className="flex flex-wrap items-center gap-3">
          <Button type="submit" disabled={save.isPending}>
            Save access
          </Button>
          <Cli command={cli} />
        </div>
      </form>
    </Section>
  );
}

const CODES: Redirect['code'][] = [301, 302, 307, 308];

function Redirects({ app, data }: Props) {
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [code, setCode] = useState<Redirect['code']>(301);
  const add = useAction(
    () => client.addRedirect(app, from.trim(), to.trim(), code),
    'Redirect added'
  );
  const remove = useAction((id: string) => client.removeRedirect(app, id), 'Redirect removed');

  return (
    <Section id="redirects" title="Redirects" meta={data.redirects.length}>
      <p className="max-w-prose text-ink-2">
        Send an old path somewhere new. Each entry matches one exact path.
      </p>
      <ul className="mt-3">
        {data.redirects.map((redirect) => (
          <Row
            key={redirect.id}
            actions={
              <ConfirmButton
                label="Remove"
                confirmLabel="Remove redirect"
                onConfirm={() => remove.mutate(redirect.id)}
              />
            }
          >
            <span className="font-mono text-[12.5px]">
              {redirect.from} <span className="text-ink-3">→</span> {redirect.to}{' '}
              <span className="text-ink-3">{redirect.code}</span>
            </span>
          </Row>
        ))}
      </ul>
      <form
        className="mt-3 flex flex-wrap items-start gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          add.mutate(undefined, {
            onSuccess: () => {
              setFrom('');
              setTo('');
            },
          });
        }}
      >
        <Field label="Path">
          <Input
            mono
            className="w-44"
            value={from}
            onChange={(e) => setFrom(e.target.value)}
            placeholder="/old-pricing"
          />
        </Field>
        <Field label="Send to" className="min-w-[200px] flex-1">
          <Input mono value={to} onChange={(e) => setTo(e.target.value)} placeholder="/pricing" />
        </Field>
        <Field label="Status">
          <Select
            value={code}
            onChange={(e) => setCode(Number(e.target.value) as Redirect['code'])}
          >
            {CODES.map((value) => (
              <option key={value} value={value}>
                {value}
                {value === 301 ? ' permanent' : value === 302 ? ' temporary' : ''}
              </option>
            ))}
          </Select>
        </Field>
        <Button
          type="submit"
          className="mt-6"
          disabled={!from.trim() || !to.trim() || add.isPending}
        >
          Add redirect
        </Button>
      </form>
      <Cli
        className="mt-3"
        command={`deku redirects add ${app} ${from.trim() || '<path>'} ${to.trim() || '<target>'} --code ${code}`}
      />
    </Section>
  );
}

function Maintenance({ app, data }: Props) {
  const toggle = useAction(
    (on: boolean) => client.setMaintenance(app, on),
    (_, on) => (on ? 'Maintenance page is on' : 'Maintenance page is off')
  );
  return (
    <Section id="maintenance" title="Maintenance">
      <div className="flex flex-wrap items-center gap-4">
        <p className="max-w-prose flex-1 text-ink-2">
          Serve a maintenance page instead of the app. Replicas keep running, so turning it off is
          instant.
        </p>
        <Button
          variant={data.maintenance ? 'primary' : 'secondary'}
          onClick={() => toggle.mutate(!data.maintenance)}
          disabled={toggle.isPending}
          aria-pressed={data.maintenance}
        >
          {data.maintenance ? 'Turn off maintenance' : 'Turn on maintenance'}
        </Button>
      </div>
      <Cli
        className="mt-3"
        command={`deku maintenance ${data.maintenance ? 'off' : 'on'} ${app}`}
      />
    </Section>
  );
}

function Processes({ app, data }: Props) {
  const scale = useAction(({ process, value }: { process: string; value: number }) =>
    client.setScale(app, process, value)
  );
  return (
    <Section
      id="processes"
      title="Processes"
      actions={<Cli command={`deku ps scale ${app} web=2`} />}
    >
      {data.processes.length === 0 ? (
        <p className="text-ink-2">
          Processes appear after the first deploy, from the Procfile or the image.
        </p>
      ) : (
        <ul>
          {data.processes.map((process) => (
            <ProcessSettings
              key={process.process_type}
              app={app}
              process={process}
              pending={scale.isPending}
              onScale={(value) => scale.mutate({ process: process.process_type, value })}
            />
          ))}
        </ul>
      )}
    </Section>
  );
}

function ProcessSettings({
  app,
  process,
  pending,
  onScale,
}: {
  app: string;
  process: ProcessRecord;
  pending: boolean;
  onScale: (value: number) => void;
}) {
  const [memory, setMemory] = useState(process.memory_limit ?? '');
  const [cpu, setCpu] = useState(process.cpu_limit?.toString() ?? '');
  const save = useAction(
    () =>
      client.setLimits(
        app,
        process.process_type,
        memory.trim() || null,
        cpu.trim() ? Number(cpu) : null
      ),
    `Limits saved for ${process.process_type}`
  );
  return (
    <li className="flex flex-col gap-3 border-b border-line py-4 last:border-b-0">
      <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
        <span className="w-[110px] font-mono font-medium">{process.process_type}</span>
        <span className="rounded-ui inline-flex items-center border border-line-2">
          <button
            type="button"
            aria-label={`Fewer ${process.process_type} replicas`}
            disabled={process.scale === 0 || pending}
            onClick={() => onScale(process.scale - 1)}
            className="inline-flex size-8 items-center justify-center text-ink-2 hover:bg-sunk disabled:opacity-40"
          >
            <Minus className="size-3.5" />
          </button>
          <span className="tabular w-9 text-center font-mono" aria-live="polite">
            {process.scale}
          </span>
          <button
            type="button"
            aria-label={`More ${process.process_type} replicas`}
            disabled={process.scale >= 10 || pending}
            onClick={() => onScale(process.scale + 1)}
            className="inline-flex size-8 items-center justify-center text-ink-2 hover:bg-sunk disabled:opacity-40"
          >
            <Plus className="size-3.5" />
          </button>
        </span>
        <span className="text-[12.5px] text-ink-3">
          {process.healthy < process.scale
            ? `Starting ${process.scale - process.healthy}…`
            : process.process_type === 'web'
              ? 'Each replica is health checked before it gets traffic'
              : `${process.scale} running`}
        </span>
      </div>
      <form
        className="flex flex-wrap items-start gap-2 sm:pl-[134px]"
        onSubmit={(event) => {
          event.preventDefault();
          save.mutate(undefined);
        }}
      >
        <Field label="Memory limit" hint="Such as 512m or 1g. Empty for none.">
          <Input
            mono
            className="w-32"
            value={memory}
            onChange={(e) => setMemory(e.target.value)}
            placeholder="none"
          />
        </Field>
        <Field label="CPU limit" hint="Cores, such as 0.5.">
          <Input
            mono
            className="w-28"
            inputMode="decimal"
            value={cpu}
            onChange={(e) => setCpu(e.target.value)}
            placeholder="none"
          />
        </Field>
        <Button type="submit" className="mt-6" size="md" disabled={save.isPending}>
          Save limits
        </Button>
      </form>
    </li>
  );
}

function Environments({ app, data }: Props) {
  const [slug, setSlug] = useState('');
  const add = useAction(
    (value: string) => client.addEnvironment(app, value),
    'Environment created'
  );
  const remove = useAction(
    (value: string) => client.removeEnvironment(app, value),
    'Environment removed'
  );
  return (
    <Section
      id="environments"
      title="Environments"
      actions={<Cli command={`deku env create ${app} staging`} />}
    >
      <p className="max-w-prose text-ink-2">
        Staging and preview targets for the same app. Each one has its own hostname, its own live
        release, and config that can override the app-wide values.
      </p>
      <ul className="mt-3">
        {data.environments.map((env) => (
          <Row
            key={env.slug}
            actions={
              env.slug === 'production' ? (
                <span className="text-[12px] text-ink-3">Always present</span>
              ) : (
                <ConfirmButton
                  label="Remove"
                  confirmLabel="Remove environment"
                  onConfirm={() => remove.mutate(env.slug)}
                />
              )
            }
          >
            <span className="block font-mono text-[13px] font-medium">{env.slug}</span>
            <span className="block truncate text-[12.5px] text-ink-3">
              {env.hostname} ·{' '}
              {env.live_release ? `release ${env.live_release}` : 'nothing deployed yet'}
            </span>
          </Row>
        ))}
        {data.environments.length === 0 ? (
          <li className="py-2 text-ink-2">Only production so far.</li>
        ) : null}
      </ul>
      <form
        className="mt-3 flex flex-wrap items-start gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          add.mutate(slug.trim(), { onSuccess: () => setSlug('') });
        }}
      >
        <Field label="Name" hint="Lowercase, such as staging or preview.">
          <Input
            mono
            className="w-48"
            value={slug}
            onChange={(e) => setSlug(e.target.value)}
            placeholder="staging"
          />
        </Field>
        <Button type="submit" className="mt-6" disabled={!slug.trim() || add.isPending}>
          Create environment
        </Button>
      </form>
    </Section>
  );
}

function DeployTokens({ app, data }: Props) {
  const now = useNow();
  const [name, setName] = useState('');
  const [secret, setSecret] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const create = useAction((label: string) => client.createToken(app, label), 'Token created');
  const revoke = useAction((id: string) => client.revokeToken(app, id), 'Token revoked');

  return (
    <Section id="tokens" title="Deploy tokens" meta={data.tokens.length}>
      <p className="max-w-prose text-ink-2">
        Let CI deploy this app, and only this app, without the dashboard token.
      </p>
      {secret ? (
        <div className="rounded-ui mt-3 border border-accent/40 bg-accent-wash/40 p-3.5">
          <p className="text-[13px] font-medium">Copy the token now. It is not shown again.</p>
          <div className="mt-2 flex flex-wrap items-center gap-2">
            <code className="rounded-ui min-w-0 flex-1 truncate border border-line bg-surface px-2.5 py-1.5 font-mono text-[12.5px]">
              {secret}
            </code>
            <Button
              size="sm"
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(secret);
                  setCopied(true);
                } catch {
                  toast('Select the token and copy it', 'warn');
                }
              }}
            >
              {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
              {copied ? 'Copied' : 'Copy token'}
            </Button>
          </div>
        </div>
      ) : null}
      <ul className="mt-3">
        {data.tokens.map((token) => (
          <Row
            key={token.id}
            actions={
              <ConfirmButton
                label="Revoke"
                confirmLabel="Revoke; CI using it fails"
                onConfirm={() => revoke.mutate(token.id)}
              />
            }
          >
            <span className="block font-medium">{token.name}</span>
            <span className="block text-[12.5px] text-ink-3">
              <span className="font-mono">{token.prefix}…</span> · created{' '}
              {relative(token.created_at, now)} ·{' '}
              {token.last_used_at ? `last used ${relative(token.last_used_at, now)}` : 'never used'}
            </span>
          </Row>
        ))}
      </ul>
      <form
        className="mt-3 flex flex-wrap items-start gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          create.mutate(name, {
            onSuccess: (result) => {
              setSecret(result.secret);
              setCopied(false);
              setName('');
            },
          });
        }}
      >
        <Field label="Name" hint="Where it will be used, such as github-actions.">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="w-56"
            placeholder="github-actions"
          />
        </Field>
        <Button type="submit" className="mt-6" disabled={!name.trim() || create.isPending}>
          Create token
        </Button>
      </form>
      <Cli
        className="mt-3"
        command={`deku deploy token create ${app} --name ${name.trim() || 'ci'}`}
      />
    </Section>
  );
}

function Storage({ app, data }: Props) {
  const store = useQuery(q.objectStore());
  const [hostPath, setHostPath] = useState('');
  const [path, setPath] = useState('');
  const [prefix, setPrefix] = useState(`apps/${app}/`);
  const addMount = useAction(
    () => client.addMount(app, hostPath.trim(), path.trim()),
    'Directory added. It is mounted from the next deploy.'
  );
  const removeMount = useAction((id: string) => client.removeMount(app, id), 'Directory removed');
  const link = useAction(() => client.linkObjectStore(app, prefix.trim()), 'Object store linked');
  const unlink = useAction(() => client.unlinkObjectStore(app), 'Object store unlinked');

  return (
    <Section id="storage" title="Storage">
      <h3 className="text-[13px] font-semibold">Persistent directories</h3>
      <p className="mt-1 max-w-prose text-ink-2">
        Files written here survive deploys and restarts. Everything else in the container is
        replaced on the next deploy.
      </p>
      <ul className="mt-2">
        {data.mounts.map((mount) => (
          <Row
            key={mount.id}
            actions={
              <ConfirmButton
                label="Remove"
                confirmLabel="Remove; files stay on disk"
                onConfirm={() => removeMount.mutate(mount.id)}
              />
            }
          >
            <span className="block font-mono text-[12.5px]">{mount.container_path}</span>
            <span className="block truncate font-mono text-[11.5px] text-ink-3">
              on the host at {mount.host_path}
            </span>
          </Row>
        ))}
      </ul>
      <form
        className="mt-3 flex flex-wrap items-start gap-3"
        onSubmit={(event) => {
          event.preventDefault();
          addMount.mutate(undefined, {
            onSuccess: () => {
              setHostPath('');
              setPath('');
            },
          });
        }}
      >
        <Field label="Directory on the server" className="min-w-[220px] flex-1">
          <Input
            mono
            value={hostPath}
            onChange={(e) => setHostPath(e.target.value)}
            placeholder={`/var/lib/deku/storage/${app}/uploads`}
          />
        </Field>
        <Field label="Path in the container" className="min-w-[180px] flex-1">
          <Input
            mono
            value={path}
            onChange={(e) => setPath(e.target.value)}
            placeholder="/app/uploads"
          />
        </Field>
        <Button
          type="submit"
          className="mt-6"
          disabled={!hostPath.trim() || !path.trim() || addMount.isPending}
        >
          Add directory
        </Button>
      </form>
      <Cli
        className="mt-3"
        command={`deku storage mount ${app} ${hostPath.trim() || '<host-path>'} ${path.trim() || '<container-path>'}`}
      />

      <h3 className="mt-8 text-[13px] font-semibold">Object store credentials</h3>
      <p className="mt-1 max-w-prose text-ink-2">
        Give the app S3 credentials for a folder in the host's object store. They appear in Config
        as S3_* variables.
      </p>
      {data.object_store ? (
        <div className="mt-3 flex flex-wrap items-center gap-3">
          <span className="font-mono text-[12.5px]">
            s3://{data.object_store.bucket}/{data.object_store.prefix}
          </span>
          <ConfirmButton
            label="Unlink"
            confirmLabel="Unlink; credentials are removed"
            onConfirm={() => unlink.mutate(undefined)}
          />
        </div>
      ) : store.data ? (
        <form
          className="mt-3 flex flex-wrap items-start gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            link.mutate(undefined);
          }}
        >
          <Field label="Folder" hint={`In the ${store.data.bucket} bucket.`}>
            <Input
              mono
              className="w-56"
              value={prefix}
              onChange={(e) => setPrefix(e.target.value)}
            />
          </Field>
          <Button type="submit" className="mt-6" disabled={link.isPending}>
            Link object store
          </Button>
        </form>
      ) : (
        <p className="mt-3 text-[13px] text-ink-3">Connect an object store in Settings first.</p>
      )}
      <Cli
        className="mt-3"
        command={`deku objectstore link ${app} --prefix ${prefix.trim() || `apps/${app}/`}`}
      />
    </Section>
  );
}

function Networks({ app, data }: Props) {
  const networks = useQuery(q.networks());
  const available = (networks.data ?? []).filter(
    (network) => network.name !== 'deku' && !data.networks.includes(network.name)
  );
  const [choice, setChoice] = useState('');
  const attach = useAction(
    (name: string) => client.attachNetwork(app, name),
    'Network attached to the running containers'
  );
  const detach = useAction((name: string) => client.detachNetwork(app, name), 'Network detached');
  const selected = choice || available[0]?.name || '';

  return (
    <Section id="networks" title="Networks">
      <p className="max-w-prose text-ink-2">
        Every app is on the private deku network with its services. Attach another network to reach
        containers Deku does not manage.
      </p>
      <ul className="mt-3">
        <Row>
          <span className="font-mono text-[12.5px]">deku</span>
          <span className="ml-2 text-[12px] text-ink-3">Always attached</span>
        </Row>
        {data.networks.map((name) => (
          <Row
            key={name}
            actions={
              <ConfirmButton
                label="Detach"
                confirmLabel="Detach network"
                onConfirm={() => detach.mutate(name)}
              />
            }
          >
            <span className="font-mono text-[12.5px]">{name}</span>
          </Row>
        ))}
      </ul>
      {available.length > 0 ? (
        <form
          className="mt-3 flex flex-wrap items-start gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (selected) attach.mutate(selected);
          }}
        >
          <Field label="Network">
            <Select value={selected} onChange={(e) => setChoice(e.target.value)}>
              {available.map((network) => (
                <option key={network.name}>{network.name}</option>
              ))}
            </Select>
          </Field>
          <Button type="submit" className="mt-6" disabled={attach.isPending}>
            Attach
          </Button>
        </form>
      ) : (
        <p className="mt-3 text-[13px] text-ink-3">
          Create a network on the Host page to attach it here.
        </p>
      )}
      <Cli className="mt-3" command={`deku network attach ${app} ${selected || '<network>'}`} />
    </Section>
  );
}

function ScheduledJobs({ app, data }: Props) {
  const [schedule, setSchedule] = useState('');
  const [command, setCommand] = useState('');
  const add = useAction(() => client.addCron(app, schedule, command), 'Job scheduled');
  const remove = useAction((id: string) => client.removeCron(app, id), 'Job removed');
  return (
    <Section id="cron" title="Scheduled jobs" meta={data.cron.length}>
      <p className="max-w-prose text-ink-2">
        Commands that run on a schedule in a fresh container from the app's image.
      </p>
      <ul className="mt-3">
        {data.cron.map((job) => (
          <Row
            key={job.id}
            actions={
              <ConfirmButton
                label="Remove"
                confirmLabel="Remove job"
                onConfirm={() => remove.mutate(job.id)}
              />
            }
          >
            <span className="font-mono text-[12.5px]">
              <span className="text-ink-3">{job.schedule}</span> {job.command}
            </span>
          </Row>
        ))}
      </ul>
      <form
        className="mt-3 flex flex-wrap items-start gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          add.mutate(undefined, {
            onSuccess: () => {
              setSchedule('');
              setCommand('');
            },
          });
        }}
      >
        <Field label="Schedule" hint="Cron syntax: minute hour day month weekday.">
          <Input
            mono
            className="w-40"
            value={schedule}
            onChange={(e) => setSchedule(e.target.value)}
            placeholder="0 3 * * *"
          />
        </Field>
        <Field label="Command" className="min-w-[220px] flex-1">
          <Input
            mono
            value={command}
            onChange={(e) => setCommand(e.target.value)}
            placeholder="bun run jobs/cleanup.ts"
          />
        </Field>
        <Button
          type="submit"
          className="mt-6"
          disabled={!schedule.trim() || !command.trim() || add.isPending}
        >
          Add job
        </Button>
      </form>
      <Cli
        className="mt-3"
        command={`deku cron add ${app} '${schedule.trim() || '0 3 * * *'}' '${command.trim() || '<command>'}'`}
      />
    </Section>
  );
}

function DangerZone({ app }: { app: string }) {
  const [typed, setTyped] = useState('');
  return (
    <Section id="danger" title="Danger zone">
      <div className="rounded-ui flex flex-col gap-3 border border-bad/35 p-4">
        <p className="text-ink-2">
          Destroying <span className="font-mono text-ink">{app}</span> removes its containers,
          images, config and deploy history. Linked services and their data are kept.
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <Input
            mono
            aria-label={`Type ${app} to confirm`}
            value={typed}
            onChange={(event) => setTyped(event.target.value)}
            placeholder={`Type ${app} to confirm`}
            className="w-[240px]"
          />
          <Button
            variant="danger"
            disabled={typed !== app}
            onClick={() => toast('The prototype does not destroy apps', 'warn')}
          >
            Destroy app
          </Button>
        </div>
        <Cli command={`deku apps destroy ${app}`} />
      </div>
    </Section>
  );
}
