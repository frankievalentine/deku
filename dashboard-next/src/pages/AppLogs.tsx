import { useQuery } from '@tanstack/react-query';
import { useNavigate, useParams, useSearch } from '@tanstack/react-router';
import { Pause, Play, Search } from 'lucide-react';
import { useMemo, useState } from 'react';
import { LogView } from '../components/LogView';
import { Button, Cli } from '../components/ui';
import { q } from '../lib/queries';
import type { LogLine } from '../lib/types';

export function AppLogs() {
  const { app } = useParams({ from: '/apps/$app' });
  const search = useSearch({ from: '/apps/$app/logs' });
  const navigate = useNavigate({ from: '/apps/$app/logs' });
  const logs = useQuery(q.logs(app));
  const [paused, setPaused] = useState<LogLine[] | null>(null);

  const source = paused ?? logs.data ?? [];
  const processes = useMemo(
    () => Array.from(new Set((logs.data ?? []).map((line) => line.process.split('.')[0]))).sort(),
    [logs.data]
  );

  const filtered = useMemo(() => {
    const needle = search.q?.toLowerCase();
    return source.filter(
      (line) =>
        (!search.stream || line.stream === search.stream) &&
        (!search.process || line.process.split('.')[0] === search.process) &&
        (!needle || line.message.toLowerCase().includes(needle))
    );
  }, [source, search.q, search.stream, search.process]);

  const behind = paused ? (logs.data?.length ?? 0) - paused.length : 0;
  const setSearch = (next: Partial<typeof search>) =>
    navigate({ search: (previous) => ({ ...previous, ...next }), replace: true });

  const cli = [`deku logs ${app}`, '--follow', search.q ? `| grep '${search.q}'` : null]
    .filter(Boolean)
    .join(' ');

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <label className="rounded-ui flex h-8 min-w-[220px] flex-1 items-center gap-2 border border-line-2 bg-surface px-2.5 focus-within:border-ink-3 sm:max-w-[360px]">
          <Search className="size-3.5 text-ink-3" />
          <span className="sr-only">Filter lines</span>
          <input
            value={search.q ?? ''}
            onChange={(event) => setSearch({ q: event.target.value || undefined })}
            placeholder="Filter lines"
            spellCheck={false}
            className="w-full bg-transparent font-mono text-[12.5px] outline-none placeholder:font-sans placeholder:text-ink-3"
          />
        </label>
        <Segmented
          label="Stream"
          value={search.stream ?? 'all'}
          options={[
            ['all', 'All'],
            ['stdout', 'stdout'],
            ['stderr', 'stderr'],
          ]}
          onChange={(value) =>
            setSearch({ stream: value === 'all' ? undefined : (value as 'stdout' | 'stderr') })
          }
        />
        <label className="rounded-ui flex h-8 items-center border border-line-2 bg-surface pr-1 pl-2.5 text-[12.5px]">
          <span className="text-ink-3">Process</span>
          <select
            value={search.process ?? ''}
            onChange={(event) => setSearch({ process: event.target.value || undefined })}
            className="h-full bg-transparent px-1.5 font-mono text-ink outline-none"
          >
            <option value="">all</option>
            {processes.map((process) => (
              <option key={process} value={process}>
                {process}
              </option>
            ))}
          </select>
        </label>
        <Button
          className="ml-auto"
          onClick={() => setPaused((value) => (value ? null : (logs.data ?? []).slice()))}
          aria-pressed={Boolean(paused)}
        >
          {paused ? <Play className="size-3.5" /> : <Pause className="size-3.5" />}
          {paused ? `Resume${behind > 0 ? ` (${behind} new)` : ''}` : 'Pause'}
        </Button>
      </div>

      <LogView
        lines={filtered}
        height="h-[calc(100dvh-330px)] min-h-[360px]"
        highlight={search.q}
        emptyText={
          search.q || search.stream || search.process
            ? 'No lines match these filters.'
            : 'No output yet.'
        }
      />

      <div className="flex flex-wrap items-center gap-3 text-[12.5px] text-ink-3">
        <span>
          Showing {filtered.length} of {source.length} recent lines
          {paused ? ', paused' : ', following live'}.
        </span>
        <Cli className="ml-auto" command={cli} />
      </div>
    </div>
  );
}

function Segmented({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: Array<[string, string]>;
  onChange: (value: string) => void;
}) {
  return (
    <fieldset className="rounded-ui flex h-8 items-center border border-line-2 bg-surface p-0.5">
      <legend className="sr-only">{label}</legend>
      {options.map(([option, text]) => (
        <label
          key={option}
          className={`rounded-ui flex h-full cursor-pointer items-center px-2.5 font-mono text-[12px] ${
            value === option ? 'bg-sunk text-ink' : 'text-ink-3 hover:text-ink'
          }`}
        >
          <input
            type="radio"
            name={label}
            value={option}
            checked={value === option}
            onChange={() => onChange(option)}
            className="sr-only"
          />
          {text}
        </label>
      ))}
    </fieldset>
  );
}
