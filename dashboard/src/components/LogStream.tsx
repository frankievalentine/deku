import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useEffectEvent, useMemo, useRef, useState } from 'react';
import {
  appEventStreamUrl,
  appLogStreamUrl,
  type EventRecord,
  getToken,
  type LogFilters,
  type LogLine,
} from '../lib/api';
import { type AppLogEntry, queryKeys, useAppLogsQuery } from '../lib/query';
import SelectField from './SelectField';

interface LogStreamProps {
  appName: string;
}

type LogTrackingState = 'live' | 'retrying' | 'not_live';

const TAIL_SIZE = 200;
const MAX_ENTRIES = 1000;

export default function LogStream({ appName }: LogStreamProps) {
  const queryClient = useQueryClient();
  const [trackingState, setTrackingState] = useState<LogTrackingState>('not_live');
  const [autoScroll, setAutoScroll] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [clearedCount, setClearedCount] = useState(0);
  const [searchInput, setSearchInput] = useState('');
  const [search, setSearch] = useState('');
  const [source, setSource] = useState('');
  const bottomRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectRef = useRef<number | null>(null);

  // Stable identity so the query key and the live append target the same cache entry.
  const filters = useMemo<LogFilters>(() => {
    const next: LogFilters = {};
    if (search.trim()) next.search = search.trim();
    if (source) next.source = source;
    return next;
  }, [search, source]);

  const logsQuery = useAppLogsQuery(appName, TAIL_SIZE, filters, {
    enabled: Boolean(appName),
  });
  const cachedEntries = logsQuery.data ?? [];
  const entries = cachedEntries.slice(Math.min(clearedCount, cachedEntries.length));
  const entryCount = entries.length;

  const handleIncomingLine = useEffectEvent((line: LogLine) => {
    // The server filters by source, but search is applied here so the live view
    // matches what the stored query would return.
    if (filters.search) {
      const haystack = line.message.toLowerCase();
      const matches = filters.search
        .toLowerCase()
        .split(/\s+/)
        .filter(Boolean)
        .every((term) => haystack.includes(term));
      if (!matches) return;
    }

    queryClient.setQueryData<AppLogEntry[]>(
      [...queryKeys.logs.app(appName, TAIL_SIZE), filters],
      (current = []) => {
        const capped = current.length >= MAX_ENTRIES ? current.slice(-(MAX_ENTRIES - 1)) : current;
        return [
          ...capped,
          {
            id: line.id,
            createdAt: line.created_at,
            eventType: line.source === 'build' ? 'build' : 'runtime',
            message: line.message,
            level: line.level,
            stream: line.stream,
            snippet: null,
          },
        ];
      }
    );
  });

  // Live log lines.
  useEffect(() => {
    let cancelled = false;
    setAutoScroll(true);
    setError(null);
    setTrackingState('not_live');
    setClearedCount(0);

    function connect() {
      if (cancelled) return;
      const token = getToken();
      const source = new EventSource(appLogStreamUrl(appName, token ?? undefined, filters));
      eventSourceRef.current = source;

      source.onopen = () => {
        setError(null);
        setTrackingState('live');
      };

      source.onmessage = (message) => {
        const line = parseLogLine(message.data);
        if (line) handleIncomingLine(line);
      };

      source.onerror = () => {
        setTrackingState('retrying');
        setError('Stream interrupted. Retrying…');
        source.close();
        if (reconnectRef.current !== null) {
          window.clearTimeout(reconnectRef.current);
        }
        reconnectRef.current = window.setTimeout(connect, 2500);
      };
    }

    connect();

    return () => {
      cancelled = true;
      eventSourceRef.current?.close();
      if (reconnectRef.current !== null) {
        window.clearTimeout(reconnectRef.current);
      }
    };
  }, [appName, filters]);

  // Deploy lifecycle events are not log lines, but the rest of the page should
  // refresh when one lands.
  const handleLifecycleEvent = useEffectEvent((event: EventRecord) => {
    if (isTerminalDeployEvent(event.event_type)) {
      void Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.apps.deployments(appName) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.apps.processes(appName) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.apps.scale(appName) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.apps.summary(appName) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.apps.list }),
        queryClient.invalidateQueries({ queryKey: queryKeys.routing.app(appName) }),
      ]);
    }
  });

  useEffect(() => {
    if (!appName) return;
    const token = getToken();
    const source = new EventSource(appEventStreamUrl(appName, token ?? undefined));
    source.onmessage = (message) => {
      const event = parseEvent(message.data);
      if (event) handleLifecycleEvent(event);
    };
    return () => source.close();
  }, [appName]);

  useEffect(() => {
    entryCount;
    if (autoScroll && bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: 'auto' });
    }
  }, [autoScroll, entryCount]);

  function handleScroll() {
    if (!containerRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = containerRef.current;
    const nearBottom = scrollHeight - scrollTop - clientHeight < 60;
    setAutoScroll(nearBottom);
  }

  const statusLabel =
    trackingState === 'live' ? 'Live' : trackingState === 'retrying' ? 'Retrying…' : 'Not live';
  const statusTone = trackingState === 'live' ? 'live' : 'danger';
  const emptyMessage =
    trackingState === 'live'
      ? 'Waiting for log lines…'
      : trackingState === 'retrying'
        ? 'Retrying live stream…'
        : (error ??
          (logsQuery.error ? 'Unable to load stored logs.' : 'Live tracking is not active.'));
  const filtered = Boolean(filters.search || filters.source);

  return (
    <div className="panel log-panel">
      <div className="log-toolbar">
        <div className="log-status">
          <span className="log-status-dot" data-state={statusTone} />
          <div className="log-status-copy">
            <span className="log-status-label" data-state={trackingState}>
              {statusLabel}
            </span>
            <span className="log-status-meta">
              {entries.length} entries{filtered ? ' (filtered)' : ''}
            </span>
          </div>
        </div>
        <div className="cluster">
          <button
            className={`btn btn-ghost btn-sm ${autoScroll ? 'is-active' : ''}`}
            onClick={() => setAutoScroll((current) => !current)}
            type="button"
          >
            {autoScroll ? 'Auto-scroll on' : 'Auto-scroll off'}
          </button>
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => setClearedCount(cachedEntries.length)}
            type="button"
          >
            Clear
          </button>
        </div>
      </div>

      <form
        className="log-filters"
        onSubmit={(event) => {
          event.preventDefault();
          setSearch(searchInput);
          setClearedCount(0);
        }}
      >
        <input
          className="input"
          type="search"
          value={searchInput}
          onChange={(event) => setSearchInput(event.target.value)}
          placeholder="Search stored logs…"
          aria-label={`Search logs for ${appName}`}
        />
        <SelectField
          id="log-source-filter"
          label="Filter logs by source"
          value={source}
          onChange={(next) => {
            setSource(next);
            setClearedCount(0);
          }}
          options={[
            { value: '', label: 'Build and runtime' },
            { value: 'runtime', label: 'Runtime only' },
            { value: 'build', label: 'Build only' },
          ]}
        />
        <button className="btn btn-secondary btn-sm" type="submit">
          Search
        </button>
        {filtered && (
          <button
            className="btn btn-ghost btn-sm"
            type="button"
            onClick={() => {
              setSearchInput('');
              setSearch('');
              setSource('');
              setClearedCount(0);
            }}
          >
            Reset
          </button>
        )}
      </form>

      <div
        ref={containerRef}
        className="log-terminal"
        onScroll={handleScroll}
        role="log"
        aria-label={`Log stream for ${appName}`}
        aria-live="polite"
        aria-atomic="false"
      >
        {entries.length === 0 && <div className="log-empty">{emptyMessage}</div>}
        {entries.map((entry, index) => (
          <div key={entry.id} className="log-line">
            <span className="log-gutter">{String(index + 1).padStart(4, '0')}</span>
            <span className="log-timestamp">
              {entry.createdAt ? formatTimestamp(entry.createdAt) : '--:--:--'}
            </span>
            <span className="log-source" data-level={entry.level}>
              [{entry.eventType}
              {entry.level ? ` ${entry.level}` : ''}]
            </span>
            <span className="log-message">
              <LogText entry={entry} search={filters.search} />
            </span>
          </div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}

/**
 * Render a log line with the matching text marked.
 *
 * Stored search results carry an FTS5 snippet with the match in brackets, which
 * is exact; live lines have no snippet, so the search terms are highlighted
 * client-side. Parts are keyed by their character offset, which is stable for a
 * given line and avoids indexing into the array.
 */
function LogText({ entry, search }: { entry: AppLogEntry; search?: string }) {
  const parts = entry.snippet
    ? snippetParts(entry.snippet)
    : termParts(
        entry.message,
        (search ?? '')
          .split(/\s+/)
          .map((term) => term.trim())
          .filter(Boolean)
      );

  return (
    <>
      {parts.map((part) =>
        part.match ? (
          <mark key={part.key}>{part.text}</mark>
        ) : (
          <span key={part.key}>{part.text}</span>
        )
      )}
    </>
  );
}

interface HighlightPart {
  key: string;
  text: string;
  match: boolean;
}

function snippetParts(snippet: string): HighlightPart[] {
  return toParts(
    snippet.split(/(\[[^\]]*\])/g),
    (segment) => segment.startsWith('[') && segment.endsWith(']')
  );
}

function termParts(message: string, terms: string[]): HighlightPart[] {
  if (terms.length === 0) {
    return [{ key: '0', text: message, match: false }];
  }
  const pattern = new RegExp(`(${terms.map(escapeRegExp).join('|')})`, 'gi');
  return toParts(message.split(pattern), (segment) =>
    terms.some((term) => term.toLowerCase() === segment.toLowerCase())
  );
}

function toParts(segments: string[], isMatch: (segment: string) => boolean): HighlightPart[] {
  const parts: HighlightPart[] = [];
  let offset = 0;
  for (const segment of segments) {
    if (segment.length === 0) continue;
    const match = isMatch(segment);
    parts.push({
      key: String(offset),
      // Snippet matches arrive wrapped in brackets; drop them when rendering.
      text:
        match && segment.startsWith('[') && segment.endsWith(']') ? segment.slice(1, -1) : segment,
      match,
    });
    offset += segment.length;
  }
  return parts;
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function parseEvent(data: string): EventRecord | null {
  try {
    return JSON.parse(data) as EventRecord;
  } catch {
    return null;
  }
}

function parseLogLine(data: string): LogLine | null {
  try {
    return JSON.parse(data) as LogLine;
  } catch {
    return null;
  }
}

function isTerminalDeployEvent(eventType: string): boolean {
  return (
    eventType === 'deploy.live' || eventType === 'deploy.failed' || eventType === 'deploy.rollback'
  );
}

function formatTimestamp(value: string): string {
  return new Date(value).toLocaleTimeString('en-US', {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}
