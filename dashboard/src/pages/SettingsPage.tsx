import { useQuery } from '@tanstack/react-query';
import { Check, Copy } from 'lucide-react';
import { type ReactNode, useState } from 'react';
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
  toast,
} from '../components/ui';
import { useAction } from '../lib/actions';
import { absolute, useNow } from '../lib/format';
import { setPrefs, type ThemeMode, usePrefs } from '../lib/prefs';
import { q } from '../lib/queries';
import type { ObjectStore } from '../lib/types';

const THEMES: Array<[ThemeMode, string]> = [
  ['system', 'System'],
  ['light', 'Light'],
  ['dark', 'Dark'],
];

const SECTIONS: Array<[string, string]> = [
  ['appearance', 'Appearance'],
  ['token', 'Dashboard token'],
  ['version', 'Version'],
  ['certificates', 'Certificates'],
  ['object-store', 'Object store'],
  ['ssh-keys', 'SSH keys'],
];

export function SettingsPage() {
  return (
    <div className="mx-auto grid max-w-[1080px] gap-10 xl:grid-cols-[minmax(0,1fr)_160px]">
      <div className="min-w-0">
        <h1 className="font-display mb-6 text-[26px] leading-tight">Settings</h1>
        <div className="flex flex-col gap-12">
          <Appearance />
          <DashboardToken />
          <Version />
          <Certificates />
          <ObjectStoreSettings />
          <SshKeys />
        </div>
      </div>
      <SectionNav items={SECTIONS} />
    </div>
  );
}

function Appearance() {
  const prefs = usePrefs();
  return (
    <Section id="appearance" title="Appearance">
      <fieldset className="flex items-center gap-2">
        <legend className="sr-only">Theme</legend>
        <span className="mr-2 text-[13px] text-ink-2">Theme</span>
        {THEMES.map(([value, label]) => (
          <label
            key={value}
            className={`rounded-ui cursor-pointer border px-3 py-1 text-[13px] ${
              prefs.theme === value
                ? 'border-ink-3 bg-sunk text-ink'
                : 'border-line text-ink-2 hover:text-ink'
            }`}
          >
            <input
              type="radio"
              name="theme"
              value={value}
              checked={prefs.theme === value}
              onChange={() => setPrefs({ theme: value })}
              className="sr-only"
            />
            {label}
          </label>
        ))}
      </fieldset>
    </Section>
  );
}

function DashboardToken() {
  const token = useQuery(q.token());
  const now = useNow();
  const [secret, setSecret] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const rotate = useAction(
    () => client.rotateToken(),
    'Token rotated. Other browsers are signed out.'
  );
  const signOut = useAction(() => client.signOut());

  const expires = token.data?.expires_at ?? null;
  const daysLeft = expires ? Math.round((new Date(expires).getTime() - now) / 86_400_000) : null;

  return (
    <Section id="token" title="Dashboard token">
      <p className="max-w-prose text-ink-2">
        This browser signed in with the dashboard token and holds a session cookie that page scripts
        cannot read. Rotating the token signs out every other browser and script that uses it.
      </p>
      {token.data ? (
        <p
          className={`mt-3 text-[13px] ${daysLeft !== null && daysLeft < 7 ? 'text-warn' : 'text-ink-2'}`}
        >
          {expires ? (
            <>
              The token expires <span title={absolute(expires)}>in {daysLeft} days</span>. Rotate it
              before then.
            </>
          ) : (
            'The token does not expire.'
          )}
        </p>
      ) : null}
      {secret ? (
        <div className="rounded-ui mt-4 border border-accent/40 bg-accent-wash/40 p-3.5">
          <p className="text-[13px] font-medium">
            Copy the new token now. It is not shown again. This browser stays signed in.
          </p>
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
      <div className="mt-4 flex flex-wrap items-center gap-3">
        <ConfirmButton
          size="md"
          variant="secondary"
          label="Rotate token"
          confirmLabel="Rotate and sign out others"
          disabled={rotate.isPending}
          onConfirm={() =>
            rotate.mutate(undefined, {
              onSuccess: (result) => {
                setSecret(result.secret);
                setCopied(false);
              },
            })
          }
        />
        <Button
          variant="ghost"
          disabled={signOut.isPending}
          onClick={() => signOut.mutate(undefined, { onSuccess: () => window.location.reload() })}
        >
          Sign out of this browser
        </Button>
        <Cli command="deku dashboard reset-token" />
      </div>
    </Section>
  );
}

function Version() {
  const host = useQuery(q.host());
  const h = host.data;
  const behind = h?.update_available ?? false;
  return (
    <Section id="version" title="Version">
      {h ? (
        <div className="flex flex-wrap items-center gap-x-8 gap-y-3">
          <dl className="flex gap-8 text-[13px]">
            <Fact label="Installed" mono>
              {h.version}
            </Fact>
            <Fact label="Latest release" mono>
              <span className={behind ? 'text-accent-ink' : ''}>
                {h.latest_version ?? 'unknown'}
              </span>
            </Fact>
          </dl>
          {behind ? (
            <div className="flex flex-col gap-1.5">
              <span className="text-[13px] text-ink-2">
                Rerun the installer on the server to update.
              </span>
              <Cli command="curl -fsSL https://get-deku.vercel.app/install.sh | sudo bash" />
            </div>
          ) : (
            <span className="text-[13px] text-ink-2">This host is on the latest release.</span>
          )}
        </div>
      ) : null}
    </Section>
  );
}

const LETS_ENCRYPT = 'https://acme-v02.api.letsencrypt.org/directory';
const LETS_ENCRYPT_STAGING = 'https://acme-staging-v02.api.letsencrypt.org/directory';

function Certificates() {
  const acme = useQuery(q.acme());
  const [email, setEmail] = useState<string | null>(null);
  const [dnsToken, setDnsToken] = useState('');
  const save = useAction(
    (next: {
      custom_domains?: boolean;
      wildcard?: boolean;
      email?: string | null;
      directory?: string;
    }) => client.setAcme(next),
    'Certificate settings saved'
  );
  const saveToken = useAction(
    (token: string) => client.setDnsToken(token),
    'Token verified and saved. Requesting the wildcard certificate.'
  );

  const a = acme.data;
  if (!a) {
    return (
      <Section id="certificates" title="Certificates">
        <div className="h-24" />
      </Section>
    );
  }

  const emailValue = email ?? a.email ?? '';
  const wildcardTone =
    a.wildcard.status === 'issued' ? 'ok' : a.wildcard.status === 'pending' ? 'busy' : 'idle';

  return (
    <Section id="certificates" title="Certificates" actions={<Cli command="deku acme status" />}>
      <div className="flex flex-wrap items-start gap-4">
        <div className="min-w-0 flex-1">
          <h3 className="text-[13px] font-semibold">App domains</h3>
          <p className="mt-1 max-w-prose text-ink-2">
            Angie requests a certificate for each domain you add and keeps it renewed. Port 80 must
            be open for the challenge.
          </p>
        </div>
        <label className="flex cursor-pointer items-center gap-2 pt-0.5 text-[13px]">
          <input
            type="checkbox"
            checked={a.custom_domains}
            disabled={save.isPending}
            onChange={(event) => save.mutate({ custom_domains: event.target.checked })}
            className="size-4 accent-[var(--accent)]"
          />
          Request certificates automatically
        </label>
      </div>

      <form
        className="mt-5 grid gap-4 sm:grid-cols-2"
        onSubmit={(event) => {
          event.preventDefault();
          save.mutate({ email: emailValue.trim() || null });
        }}
      >
        <Field label="Certificate email" hint="Let's Encrypt's contact for this host's account.">
          <Input
            type="email"
            value={emailValue}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="ops@example.com"
          />
        </Field>
        <div className="flex flex-col gap-1.5">
          <span className="text-[12.5px] leading-[18px] font-medium">Global domain</span>
          <span className="rounded-ui flex h-8 items-center border border-line bg-sunk px-2.5 font-mono text-[12.5px] text-ink-2">
            {a.global_domain ?? 'not set'}
          </span>
          <span className="text-[12px] text-ink-3">
            Set as global_domain in the daemon config. Apps get hostnames under it.
          </span>
        </div>
        <div className="sm:col-span-2">
          <Button type="submit" disabled={save.isPending}>
            Save email
          </Button>
        </div>
      </form>

      <div className="mt-5 grid gap-4 sm:grid-cols-2">
        {a.directory === LETS_ENCRYPT || a.directory === LETS_ENCRYPT_STAGING ? (
          <Field
            label="Certificate authority"
            hint={
              a.directory === LETS_ENCRYPT_STAGING
                ? 'Staging certificates are not trusted by browsers. Switch back once issuance works.'
                : 'Staging has much higher rate limits, for testing a new setup.'
            }
          >
            <Select
              value={a.directory}
              disabled={save.isPending}
              onChange={(event) => save.mutate({ directory: event.target.value })}
            >
              <option value={LETS_ENCRYPT}>Let's Encrypt</option>
              <option value={LETS_ENCRYPT_STAGING}>Let's Encrypt staging (untrusted)</option>
            </Select>
          </Field>
        ) : (
          <div className="flex flex-col gap-1.5">
            <span className="text-[12.5px] leading-[18px] font-medium">Certificate authority</span>
            <span className="rounded-ui flex h-8 items-center truncate border border-line bg-sunk px-2.5 font-mono text-[12.5px] text-ink-2">
              {a.directory}
            </span>
            <span className="text-[12px] text-ink-3">
              A custom ACME directory, set as directory in the daemon config.
            </span>
          </div>
        )}
      </div>

      <div className="mt-6 border-t border-line pt-4">
        <div className="flex flex-wrap items-center gap-3">
          <h3 className="text-[13px] font-semibold">Wildcard certificate</h3>
          <span className="inline-flex items-center gap-1.5 text-[12.5px] text-ink-2">
            <Dot tone={wildcardTone} pulse={a.wildcard.status === 'pending'} />
            {a.wildcard.status === 'issued'
              ? `Issued for *.${a.global_domain}`
              : a.wildcard.status === 'pending'
                ? 'Requesting'
                : 'Off'}
          </span>
          <label className="ml-auto flex cursor-pointer items-center gap-2 text-[13px]">
            <input
              type="checkbox"
              checked={a.wildcard.enabled}
              disabled={save.isPending || (!a.wildcard.token_set && !a.wildcard.enabled)}
              onChange={(event) => save.mutate({ wildcard: event.target.checked })}
              className="size-4 accent-[var(--accent)]"
            />
            Request and renew automatically
          </label>
        </div>
        <p className="mt-1.5 max-w-prose text-ink-2">
          One certificate covers every hostname Deku generates under{' '}
          {a.global_domain ?? 'the global domain'}, including staging and preview addresses, and the
          dashboard if you publish it. Deku answers the DNS challenge through Cloudflare.
        </p>
        <form
          className="mt-3 flex flex-wrap items-start gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            saveToken.mutate(dnsToken, { onSuccess: () => setDnsToken('') });
          }}
        >
          <Field
            label={
              a.wildcard.token_set ? 'Replace the Cloudflare API token' : 'Cloudflare API token'
            }
            hint="Needs Zone:Read and DNS:Edit for the zone."
            className="min-w-[260px] flex-1"
          >
            <Input
              mono
              type="password"
              value={dnsToken}
              onChange={(e) => setDnsToken(e.target.value)}
              autoComplete="off"
            />
          </Field>
          <Button type="submit" className="mt-6" disabled={!dnsToken.trim() || saveToken.isPending}>
            Verify and save
          </Button>
        </form>
      </div>
    </Section>
  );
}

const PROVIDERS = [
  'Cloudflare R2',
  'AWS S3',
  'Backblaze B2',
  'DigitalOcean Spaces',
  'MinIO',
  'Other S3-compatible',
];

function ObjectStoreSettings() {
  const store = useQuery(q.objectStore());
  const [editing, setEditing] = useState(false);
  const test = useAction(
    () => client.testObjectStore(),
    (result) => result.detail
  );
  const remove = useAction(() => client.removeObjectStore(), 'Object store removed');

  return (
    <Section
      id="object-store"
      title="Object store"
      actions={
        store.data ? (
          <Cli command="deku objectstore test" />
        ) : (
          <Cli command="deku objectstore setup" />
        )
      }
    >
      <p className="max-w-prose text-ink-2">
        Backups and retained deploy artifacts are written here. Any S3-compatible storage works.
      </p>
      {store.data && !editing ? (
        <>
          <dl className="mt-4 grid grid-cols-2 gap-x-6 gap-y-3 text-[13px] sm:grid-cols-4">
            <Fact label="Provider">{store.data.provider}</Fact>
            <Fact label="Bucket" mono>
              {store.data.bucket}
            </Fact>
            <Fact label="Region" mono>
              {store.data.region}
            </Fact>
            <Fact label="Access key" mono>
              {store.data.access_key_id}
            </Fact>
            <div className="col-span-full">
              <dt className="text-[12px] text-ink-3">Endpoint</dt>
              <dd className="mt-0.5 font-mono text-[12.5px] break-all">{store.data.endpoint}</dd>
            </div>
          </dl>
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <Button onClick={() => test.mutate(undefined)} disabled={test.isPending}>
              {test.isPending ? 'Testing…' : 'Test connection'}
            </Button>
            <Button variant="ghost" onClick={() => setEditing(true)}>
              Change
            </Button>
            <ConfirmButton
              label="Remove"
              confirmLabel="Remove; backups stop"
              onConfirm={() => remove.mutate(undefined)}
            />
          </div>
        </>
      ) : (
        <ObjectStoreForm initial={store.data ?? null} onDone={() => setEditing(false)} />
      )}
    </Section>
  );
}

function ObjectStoreForm({ initial, onDone }: { initial: ObjectStore | null; onDone: () => void }) {
  const [draft, setDraft] = useState<ObjectStore>(
    initial ?? {
      provider: PROVIDERS[0],
      endpoint: '',
      bucket: '',
      region: 'auto',
      access_key_id: '',
      path_style: false,
    }
  );
  const [secret, setSecret] = useState('');
  const save = useAction(
    ({ next, key }: { next: ObjectStore; key: string }) => client.setObjectStore(next, key),
    'Object store saved'
  );
  const set =
    (key: 'provider' | 'endpoint' | 'bucket' | 'region' | 'access_key_id') =>
    (event: { target: { value: string } }) =>
      setDraft((value) => ({ ...value, [key]: event.target.value }));

  return (
    <form
      className="mt-4 grid gap-4 sm:grid-cols-2"
      onSubmit={(event) => {
        event.preventDefault();
        save.mutate({ next: draft, key: secret }, { onSuccess: onDone });
      }}
    >
      <Field label="Provider">
        <Select value={draft.provider} onChange={set('provider')}>
          {PROVIDERS.map((provider) => (
            <option key={provider}>{provider}</option>
          ))}
        </Select>
      </Field>
      <Field label="Endpoint" hint="The https:// URL from your provider.">
        <Input
          mono
          value={draft.endpoint}
          onChange={set('endpoint')}
          placeholder="https://s3.us-east-1.amazonaws.com"
        />
      </Field>
      <Field label="Bucket">
        <Input mono value={draft.bucket} onChange={set('bucket')} placeholder="deku-backups" />
      </Field>
      <Field label="Region">
        <Input mono value={draft.region} onChange={set('region')} placeholder="us-east-1" />
      </Field>
      <Field label="Access key ID">
        <Input
          mono
          value={draft.access_key_id}
          onChange={set('access_key_id')}
          autoComplete="off"
        />
      </Field>
      <Field label="Secret access key" hint="Required each time the settings are saved.">
        <Input
          mono
          type="password"
          value={secret}
          onChange={(e) => setSecret(e.target.value)}
          autoComplete="off"
        />
      </Field>
      <label className="flex items-center gap-2 text-[13px] sm:col-span-2">
        <input
          type="checkbox"
          checked={draft.path_style}
          onChange={(e) => setDraft((value) => ({ ...value, path_style: e.target.checked }))}
          className="size-4 accent-[var(--accent)]"
        />
        Use path-style URLs (MinIO and some self-hosted stores need this)
      </label>
      <div className="flex gap-2 sm:col-span-2">
        <Button type="submit" variant="primary" disabled={!secret || save.isPending}>
          Save object store
        </Button>
        {initial ? (
          <Button variant="ghost" onClick={onDone}>
            Cancel
          </Button>
        ) : null}
      </div>
    </form>
  );
}

function SshKeys() {
  const keys = useQuery(q.sshKeys());
  const [name, setName] = useState('');
  const [publicKey, setPublicKey] = useState('');
  const add = useAction(
    ({ label, key }: { label: string; key: string }) => client.addSshKey(label, key),
    'Key added'
  );
  const remove = useAction((keyName: string) => client.removeSshKey(keyName), 'Key removed');

  return (
    <Section
      id="ssh-keys"
      title="SSH keys"
      meta={keys.data?.length ?? 0}
      actions={<Cli command="deku ssh add <name> <key-or-path>" />}
    >
      <p className="max-w-prose text-ink-2">These keys can deploy with git push over SSH.</p>
      <ul className="mt-3">
        {(keys.data ?? []).map((key) => (
          <Row
            key={key.name}
            actions={
              <ConfirmButton
                label="Remove"
                confirmLabel="Remove key"
                onConfirm={() => remove.mutate(key.name)}
              />
            }
          >
            <span className="block font-medium">{key.name}</span>
            <span className="block truncate font-mono text-[12px] text-ink-3">
              {key.fingerprint ?? 'fingerprint unavailable'}
            </span>
          </Row>
        ))}
        {keys.data?.length === 0 ? <li className="py-2 text-ink-2">No keys yet.</li> : null}
      </ul>
      <form
        className="mt-4 grid gap-3 sm:grid-cols-[200px_minmax(0,1fr)_auto] sm:items-start"
        onSubmit={(event) => {
          event.preventDefault();
          add.mutate(
            { label: name, key: publicKey },
            {
              onSuccess: () => {
                setName('');
                setPublicKey('');
              },
            }
          );
        }}
      >
        <Field label="Name">
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="laptop" />
        </Field>
        <Field label="Public key">
          <Input
            mono
            value={publicKey}
            onChange={(e) => setPublicKey(e.target.value)}
            placeholder="ssh-ed25519 AAAA… you@laptop"
          />
        </Field>
        <Button
          type="submit"
          className="sm:mt-6"
          disabled={!name.trim() || !publicKey.trim() || add.isPending}
        >
          Add key
        </Button>
      </form>
    </Section>
  );
}

function Fact({
  label,
  mono = false,
  children,
}: {
  label: string;
  mono?: boolean;
  children: ReactNode;
}) {
  return (
    <div className="min-w-0">
      <dt className="text-[12px] text-ink-3">{label}</dt>
      <dd className={`mt-0.5 truncate ${mono ? 'font-mono text-[12.5px]' : ''}`}>{children}</dd>
    </div>
  );
}
