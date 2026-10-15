import { useQuery } from '@tanstack/react-query';
import { Link, useParams } from '@tanstack/react-router';
import { Eye, EyeOff, Link2, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { Button, Cli, Empty, Input, Section } from '../components/ui';
import { useAction } from '../lib/actions';
import { q } from '../lib/queries';
import type { ConfigVar } from '../lib/types';
import { client } from '../mock/api';

/** `undefined` is the app-wide scope; a string is one environment's overrides. */
type Scope = string | undefined;

export function AppConfig() {
  const { app } = useParams({ from: '/apps/$app' });
  const config = useQuery(q.config(app));
  const detail = useQuery(q.app(app));
  const [scope, setScope] = useState<Scope>(undefined);
  const [changed, setChanged] = useState(false);

  const vars = config.data ?? [];
  const environments = (detail.data?.environments ?? []).filter((env) => env.slug !== 'production');
  const linked = vars.filter((entry) => entry.service);
  const appWide = vars.filter((entry) => !entry.service && !entry.environment);
  const overrides = scope ? vars.filter((entry) => entry.environment === scope) : [];
  const overridden = new Set(overrides.map((entry) => entry.key));
  const markChanged = () => setChanged(true);

  return (
    <div className="flex flex-col gap-8">
      {changed ? (
        <p className="rounded-ui flex flex-wrap items-center gap-3 border border-warn/40 bg-warn/[0.07] px-3.5 py-2.5 text-[13px]">
          Config changes apply to new containers. Redeploy to use them in the running release.
          <Link to="/apps/$app" params={{ app }} className="ml-auto font-medium underline">
            Go to redeploy
          </Link>
        </p>
      ) : null}

      {environments.length > 0 ? (
        <fieldset className="rounded-ui flex w-fit items-center border border-line-2 bg-surface p-0.5">
          <legend className="sr-only">Which values to show</legend>
          {[undefined, ...environments.map((env) => env.slug)].map((value) => (
            <label
              key={value ?? 'app-wide'}
              className={`rounded-ui cursor-pointer px-3 py-1 text-[13px] ${
                scope === value ? 'bg-sunk font-medium text-ink' : 'text-ink-3 hover:text-ink'
              }`}
            >
              <input
                type="radio"
                name="scope"
                checked={scope === value}
                onChange={() => setScope(value)}
                className="sr-only"
              />
              {value ? <span className="font-mono">{value}</span> : 'App-wide'}
            </label>
          ))}
        </fieldset>
      ) : null}

      {scope ? (
        <Section
          title={`Overrides for ${scope}`}
          meta={overrides.length}
          actions={<Cli command={`deku config set ${app} KEY=value --environment ${scope}`} />}
        >
          <p className="mb-3 max-w-prose text-ink-2">
            These replace the app-wide value of the same key in {scope}. Everything else is
            inherited.
          </p>
          <AddVar app={app} scope={scope} onChanged={markChanged} />
          <ul className="mt-3 border-t border-line">
            {overrides.map((entry) => (
              <VarRow key={entry.key} app={app} entry={entry} onChanged={markChanged} />
            ))}
            {appWide
              .filter((entry) => !overridden.has(entry.key))
              .map((entry) => (
                <VarRow key={entry.key} app={app} entry={entry} inherited onChanged={markChanged} />
              ))}
          </ul>
        </Section>
      ) : (
        <Section
          title="Variables"
          meta={appWide.length}
          actions={<Cli command={`deku config set ${app} KEY=value`} />}
        >
          <AddVar app={app} scope={undefined} onChanged={markChanged} />
          {appWide.length === 0 ? (
            <div className="mt-3">
              <Empty title="No variables set">
                Values are encrypted at rest and given to every process of the app.
              </Empty>
            </div>
          ) : (
            <ul className="mt-3 border-t border-line">
              {appWide.map((entry) => (
                <VarRow key={entry.key} app={app} entry={entry} onChanged={markChanged} />
              ))}
            </ul>
          )}
        </Section>
      )}

      <Section title="Set by Deku" meta={linked.length}>
        {linked.length === 0 ? (
          <p className="text-ink-2">
            Link a service or the object store and its connection values appear here.
          </p>
        ) : (
          <ul className="border-t border-line">
            {linked.map((entry) => (
              <VarRow
                key={`${entry.service}-${entry.key}`}
                app={app}
                entry={entry}
                onChanged={markChanged}
              />
            ))}
          </ul>
        )}
      </Section>
    </div>
  );
}

function VarRow({
  app,
  entry,
  inherited = false,
  onChanged,
}: {
  app: string;
  entry: ConfigVar;
  inherited?: boolean;
  onChanged: () => void;
}) {
  const [revealed, setRevealed] = useState(false);
  const unset = useAction(
    () => client.unsetEnvConfig(app, entry.environment, entry.key),
    `Removed ${entry.key}`
  );
  return (
    <li
      className={`group grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1 border-b border-line py-2 sm:grid-cols-[minmax(160px,260px)_minmax(0,1fr)_auto] ${
        inherited ? 'opacity-60' : ''
      }`}
    >
      <span className="truncate font-mono text-[12.5px] font-medium">{entry.key}</span>
      <span className="col-span-2 row-start-2 min-w-0 truncate font-mono text-[12.5px] text-ink-2 sm:col-span-1 sm:row-auto">
        {revealed ? entry.value : '•'.repeat(Math.min(24, Math.max(10, entry.value.length)))}
      </span>
      <span className="col-start-2 row-start-1 flex items-center gap-1 justify-self-end sm:col-auto sm:row-auto">
        {inherited ? <span className="mr-1 text-[12px] text-ink-3">app-wide</span> : null}
        {entry.service === 'object store' ? (
          <Link
            to="/apps/$app/settings"
            params={{ app }}
            hash="storage"
            className="mr-1 inline-flex items-center gap-1 text-[12px] text-ink-3 hover:text-ink"
          >
            <Link2 className="size-3.5" />
            object store
          </Link>
        ) : entry.service ? (
          <Link
            to="/services"
            className="mr-1 inline-flex items-center gap-1 text-[12px] text-ink-3 hover:text-ink"
          >
            <Link2 className="size-3.5" />
            {entry.service}
          </Link>
        ) : null}
        <button
          type="button"
          onClick={() => setRevealed((value) => !value)}
          aria-label={revealed ? `Hide ${entry.key}` : `Reveal ${entry.key}`}
          className="rounded-ui inline-flex size-7 items-center justify-center text-ink-3 hover:bg-sunk hover:text-ink"
        >
          {revealed ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
        </button>
        {entry.service || inherited ? null : (
          <button
            type="button"
            onClick={() => unset.mutate(undefined, { onSuccess: onChanged })}
            aria-label={`Remove ${entry.key}`}
            className="rounded-ui inline-flex size-7 items-center justify-center text-ink-3 opacity-60 group-hover:opacity-100 hover:bg-sunk hover:text-bad"
          >
            <Trash2 className="size-3.5" />
          </button>
        )}
      </span>
    </li>
  );
}

function AddVar({ app, scope, onChanged }: { app: string; scope: Scope; onChanged: () => void }) {
  const [key, setKey] = useState('');
  const [value, setValue] = useState('');
  const set = useAction(
    ({ k, v }: { k: string; v: string }) => client.setEnvConfig(app, scope, k, v),
    (_, { k }) => `Set ${k}`
  );
  const validKey = /^[A-Za-z_][A-Za-z0-9_]*$/.test(key.trim());
  return (
    <form
      className="flex flex-wrap items-center gap-2"
      onSubmit={(event) => {
        event.preventDefault();
        if (!validKey) return;
        set.mutate(
          { k: key.trim(), v: value },
          {
            onSuccess: () => {
              setKey('');
              setValue('');
              onChanged();
            },
          }
        );
      }}
    >
      <Input
        mono
        aria-label="Key"
        value={key}
        onChange={(event) => setKey(event.target.value.toUpperCase())}
        placeholder="KEY"
        className="w-[220px]"
      />
      <Input
        mono
        aria-label="Value"
        value={value}
        onChange={(event) => setValue(event.target.value)}
        placeholder="value"
        className="min-w-[200px] flex-1"
      />
      <Button type="submit" disabled={!validKey || set.isPending}>
        {scope ? `Set for ${scope}` : 'Set'}
      </Button>
    </form>
  );
}
