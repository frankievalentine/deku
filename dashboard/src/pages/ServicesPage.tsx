import { useQuery } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { ChevronDown, Eye, EyeOff, Plus } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { client } from '../api/client';
import {
  Button,
  Cli,
  ConfirmButton,
  Dot,
  Empty,
  Field,
  Input,
  Row,
  Select,
} from '../components/ui';
import { useAction } from '../lib/actions';
import { absolute, relative, useNow } from '../lib/format';
import { q } from '../lib/queries';
import type { ManagedService, ServiceKind } from '../lib/types';

const KIND_LABEL: Record<ServiceKind, string> = {
  postgres: 'Postgres',
  mysql: 'MySQL',
  mariadb: 'MariaDB',
  redis: 'Redis',
  mongodb: 'MongoDB',
};

const KINDS = Object.keys(KIND_LABEL) as ServiceKind[];

export function ServicesPage() {
  const services = useQuery(q.services());
  const [creating, setCreating] = useState(false);
  const list = services.data ?? [];
  const unprotected = list.filter((service) => !service.backup_schedule).length;

  return (
    <div className="mx-auto max-w-[1280px]">
      <header className="mb-5 flex flex-wrap items-end gap-x-4 gap-y-3">
        <div>
          <h1 className="font-display text-[26px] leading-tight">Services</h1>
          <p className="mt-0.5 text-ink-2">
            {list.length} managed services on the private deku network.
            {unprotected > 0 ? (
              <span className="text-warn"> {unprotected} without a backup schedule.</span>
            ) : null}
          </p>
        </div>
        <Button
          variant="primary"
          className="ml-auto"
          onClick={() => setCreating((value) => !value)}
          aria-expanded={creating}
        >
          <Plus className="size-4" />
          New service
        </Button>
      </header>

      {creating ? <CreateService onDone={() => setCreating(false)} /> : null}

      {services.isSuccess && list.length === 0 ? (
        <Empty title="No services yet" command="deku postgres create <name>">
          Postgres, MySQL, MariaDB, Redis and MongoDB run as containers with their own volume. Link
          one to an app and its connection URL is set for you.
        </Empty>
      ) : (
        <ul className="border-t border-line">
          {list.map((service) => (
            <ServiceRow key={service.name} service={service} />
          ))}
        </ul>
      )}
    </div>
  );
}

function CreateService({ onDone }: { onDone: () => void }) {
  const [kind, setKind] = useState<ServiceKind>('postgres');
  const [name, setName] = useState('');
  const create = useAction(
    () => client.createService(kind, name.trim()),
    (service) => `Created ${service.name}`
  );
  return (
    <form
      className="rounded-ui mb-6 flex flex-wrap items-start gap-3 border border-line bg-surface p-3.5"
      onSubmit={(event) => {
        event.preventDefault();
        create.mutate(undefined, { onSuccess: onDone });
      }}
    >
      <Field label="Type">
        <Select value={kind} onChange={(e) => setKind(e.target.value as ServiceKind)}>
          {KINDS.map((value) => (
            <option key={value} value={value}>
              {KIND_LABEL[value]}
            </option>
          ))}
        </Select>
      </Field>
      <Field
        label="Name"
        hint="Lowercase letters, digits and dashes."
        className="min-w-[220px] flex-1"
      >
        <Input
          mono
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="shop-db"
        />
      </Field>
      <div className="flex gap-2">
        <Button
          type="submit"
          className="mt-6"
          variant="primary"
          disabled={!name.trim() || create.isPending}
        >
          {create.isPending ? 'Creating…' : 'Create service'}
        </Button>
        <Button variant="ghost" onClick={onDone}>
          Cancel
        </Button>
      </div>
      <div className="basis-full">
        <Cli command={`deku ${kind} create ${name.trim() || '<name>'}`} />
      </div>
    </form>
  );
}

function ServiceRow({ service }: { service: ManagedService }) {
  const [open, setOpen] = useState(false);
  const now = useNow();
  const stale =
    !service.last_backup_at || now - new Date(service.last_backup_at).getTime() > 2 * 86_400_000;

  return (
    <li className="border-b border-line">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        className="grid w-full grid-cols-[1fr_auto] items-center gap-x-5 gap-y-1 px-2 py-3 text-left hover:bg-sunk/60 md:grid-cols-[minmax(200px,1.2fr)_minmax(140px,1fr)_minmax(160px,1fr)_20px]"
      >
        <span className="flex min-w-0 items-center gap-3">
          <Dot
            tone={service.status === 'running' ? 'ok' : service.status === 'error' ? 'bad' : 'idle'}
          />
          <span className="min-w-0">
            <span className="block truncate font-mono text-[13.5px] font-medium">
              {service.name}
            </span>
            <span className="block text-[12.5px] text-ink-3">{KIND_LABEL[service.kind]}</span>
          </span>
        </span>
        <span className="hidden min-w-0 truncate text-[12.5px] text-ink-2 md:block">
          {service.linked_apps.length === 0 ? (
            <span className="text-ink-3">Not linked</span>
          ) : (
            service.linked_apps.join(', ')
          )}
        </span>
        <span
          className={`col-span-2 text-[12.5px] md:col-span-1 ${stale ? 'text-warn' : 'text-ink-2'}`}
        >
          {service.last_backup_at
            ? `Backed up ${relative(service.last_backup_at, now)}`
            : 'Never backed up'}
          <span className="block text-ink-3">
            {service.backup_schedule
              ? `Every ${service.backup_schedule.interval_hours}h, keeping ${service.backup_schedule.keep}`
              : 'No schedule'}
          </span>
        </span>
        <ChevronDown
          aria-hidden="true"
          className={`col-start-2 row-start-1 size-4 text-ink-3 transition-transform md:col-auto md:row-auto ${open ? 'rotate-180' : ''}`}
        />
      </button>
      {open ? <ServiceDetail service={service} /> : null}
    </li>
  );
}

function ServiceDetail({ service }: { service: ManagedService }) {
  return (
    <div className="grid gap-8 px-2 pt-2 pb-6 md:pl-[30px] lg:grid-cols-2">
      <div className="flex min-w-0 flex-col gap-8">
        <Connection service={service} />
        <LinkedApps service={service} />
      </div>
      <div className="flex min-w-0 flex-col gap-8">
        <Backups service={service} />
        <Output service={service} />
      </div>
    </div>
  );
}

function Subheading({ children, action }: { children: string; action?: ReactNode }) {
  return (
    <div className="mb-2 flex items-center gap-3 border-b border-line pb-1.5">
      <h3 className="text-[12.5px] font-semibold">{children}</h3>
      {action ? <div className="ml-auto min-w-0">{action}</div> : null}
    </div>
  );
}

function Connection({ service }: { service: ManagedService }) {
  const connection = useQuery(q.connection(service.name));
  const [revealed, setRevealed] = useState(false);
  const c = connection.data;
  return (
    <section>
      <Subheading action={<Cli command={`deku ${service.kind} info ${service.name}`} />}>
        Connection
      </Subheading>
      {c ? (
        <dl className="flex flex-col gap-2 text-[13px]">
          <div>
            <dt className="text-[12px] text-ink-3">Reachable from apps at</dt>
            <dd className="mt-0.5 font-mono text-[12.5px]">
              {c.host}:{c.port}
            </dd>
          </div>
          <div>
            <dt className="text-[12px] text-ink-3">URL</dt>
            <dd className="mt-0.5 flex items-center gap-2">
              <span className="min-w-0 truncate font-mono text-[12.5px]">
                {revealed ? c.url : c.url.replace(/:[^:@/]+@/, ':••••••@')}
              </span>
              <button
                type="button"
                onClick={() => setRevealed((value) => !value)}
                aria-label={revealed ? 'Hide password' : 'Reveal password'}
                className="rounded-ui inline-flex size-7 shrink-0 items-center justify-center text-ink-3 hover:bg-sunk hover:text-ink"
              >
                {revealed ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
              </button>
            </dd>
          </div>
          <p className="text-[12.5px] text-ink-3">
            The service publishes no port on the host. To open a client session, run{' '}
            <span className="font-mono">
              deku {service.kind} connect {service.name}
            </span>{' '}
            on the server.
          </p>
        </dl>
      ) : null}
    </section>
  );
}

function LinkedApps({ service }: { service: ManagedService }) {
  const apps = useQuery(q.apps());
  const candidates = (apps.data ?? [])
    .map((entry) => entry.app.name)
    .filter((name) => !service.linked_apps.includes(name));
  const [choice, setChoice] = useState('');
  const selected = choice && candidates.includes(choice) ? choice : (candidates[0] ?? '');
  const link = useAction(
    (app: string) => client.linkService(service.name, app),
    (_, app) => `Linked to ${app}. Redeploy it to use the new URL.`
  );
  const unlink = useAction(
    (app: string) => client.unlinkService(service.name, app),
    (_, app) => `Unlinked from ${app}`
  );

  return (
    <section>
      <Subheading>Linked apps</Subheading>
      <ul>
        {service.linked_apps.map((app) => (
          <Row
            key={app}
            actions={
              <ConfirmButton
                label="Unlink"
                confirmLabel="Unlink; the app loses its URL"
                onConfirm={() => unlink.mutate(app)}
              />
            }
          >
            <Link
              to="/apps/$app/config"
              params={{ app }}
              className="font-mono text-[12.5px] hover:underline"
            >
              {app}
            </Link>
          </Row>
        ))}
        {service.linked_apps.length === 0 ? (
          <li className="py-1 text-[13px] text-ink-2">No app uses this service yet.</li>
        ) : null}
      </ul>
      {candidates.length > 0 ? (
        <form
          className="mt-3 flex items-start gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (selected) link.mutate(selected);
          }}
        >
          <Field label="Link to an app">
            <Select value={selected} onChange={(e) => setChoice(e.target.value)}>
              {candidates.map((name) => (
                <option key={name}>{name}</option>
              ))}
            </Select>
          </Field>
          <Button type="submit" className="mt-6" disabled={link.isPending}>
            Link
          </Button>
        </form>
      ) : null}
      <Cli
        className="mt-3"
        command={`deku ${service.kind} link ${service.name} ${selected || '<app>'}`}
      />
    </section>
  );
}

const INTERVALS = [6, 12, 24, 168];
const KEEP = [3, 7, 14, 30];

function Backups({ service }: { service: ManagedService }) {
  const backups = useQuery(q.backups(service.name));
  const now = useNow();
  const [every, setEvery] = useState(service.backup_schedule?.interval_hours ?? 24);
  const [keep, setKeep] = useState(service.backup_schedule?.keep ?? 7);
  const backup = useAction(() => client.backupService(service.name), `Backing up ${service.name}`);
  const restore = useAction(
    (id: string) => client.restoreBackup(service.name, id),
    (_, id) => `Restoring ${service.name} from ${id}`
  );
  const schedule = useAction(
    (next: ManagedService['backup_schedule']) => client.setBackupSchedule(service.name, next),
    (_, next) => (next ? 'Backup schedule saved' : 'Automatic backups are off')
  );

  return (
    <section>
      <Subheading
        action={
          <Button size="sm" onClick={() => backup.mutate(undefined)} disabled={backup.isPending}>
            Back up now
          </Button>
        }
      >
        Backups
      </Subheading>
      <ul>
        {(backups.data ?? []).map((entry) => (
          <Row
            key={entry.id}
            actions={
              <ConfirmButton
                label="Restore"
                confirmLabel="Replace current data"
                onConfirm={() => restore.mutate(entry.id)}
              />
            }
          >
            <span className="font-mono text-[12.5px]">{entry.id}</span>
            <span className="ml-2 text-[12.5px] text-ink-3" title={absolute(entry.created_at)}>
              {relative(entry.created_at, now)} · {entry.size}
            </span>
          </Row>
        ))}
        {backups.data?.length === 0 ? (
          <li className="py-1 text-[13px] text-ink-2">No backups yet.</li>
        ) : null}
      </ul>
      <form
        className="mt-4 flex flex-wrap items-start gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          schedule.mutate({ interval_hours: every, keep });
        }}
      >
        <Field label="Back up every">
          <Select value={every} onChange={(e) => setEvery(Number(e.target.value))}>
            {INTERVALS.map((hours) => (
              <option key={hours} value={hours}>
                {hours === 168 ? 'week' : hours === 24 ? 'day' : `${hours} hours`}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Keep">
          <Select value={keep} onChange={(e) => setKeep(Number(e.target.value))}>
            {KEEP.map((count) => (
              <option key={count} value={count}>
                {count} backups
              </option>
            ))}
          </Select>
        </Field>
        <Button type="submit" className="mt-6" disabled={schedule.isPending}>
          {service.backup_schedule ? 'Update schedule' : 'Turn on automatic backups'}
        </Button>
        {service.backup_schedule ? (
          <ConfirmButton
            size="md"
            className="mt-6"
            label="Turn off"
            confirmLabel="Stop automatic backups"
            onConfirm={() => schedule.mutate(null)}
          />
        ) : null}
      </form>
      <Cli
        className="mt-3"
        command={`deku backup schedule ${service.name} --interval-hours ${every} --keep ${keep}`}
      />
    </section>
  );
}

function Output({ service }: { service: ManagedService }) {
  const logs = useQuery(q.serviceLogs(service.name));
  return (
    <section>
      <Subheading action={<Cli command={`deku ${service.kind} logs ${service.name}`} />}>
        Recent output
      </Subheading>
      <div className="rounded-ui bg-term px-3.5 py-2.5 font-mono text-[12px] leading-[1.65] text-term-ink/85">
        {(logs.data ?? []).map((line) => (
          <div key={line} className="break-words">
            {line}
          </div>
        ))}
      </div>
    </section>
  );
}
