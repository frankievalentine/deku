import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useEffectEvent, useRef, useState } from 'react';
import { appEventStreamUrl, type EventRecord, getToken } from '../lib/api';
import { type AppLogEntry, queryKeys, useAppLogsQuery } from '../lib/query';

interface LogStreamProps {
  appName: string;
}

type LogTrackingState = 'live' | 'retrying' | 'not_live';

export default function LogStream({ appName }: LogStreamProps) {
  const queryClient = useQueryClient();
  const [trackingState, setTrackingState] = useState<LogTrackingState>('not_live');
  const [autoScroll, setAutoScroll] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [clearedCount, setClearedCount] = useState(0);
  const bottomRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectRef = useRef<number | null>(null);
  const logsQuery = useAppLogsQuery(appName, 120, { enabled: Boolean(appName) });
  const cachedEntries = logsQuery.data ?? [];
  const entries = cachedEntries.slice(Math.min(clearedCount, cachedEntries.length));
  const entryCount = entries.length;
  const handleIncomingEvent = useEffectEvent((event: EventRecord) => {
    const nextEntry = eventToEntry(event);
    if (nextEntry) {
      queryClient.setQueryData<AppLogEntry[]>(queryKeys.logs.app(appName, 120), (current = []) => {
        const capped = current.length >= 500 ? current.slice(-499) : current;
        return [...capped, nextEntry];
      });
    }

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
    let cancelled = false;
    setAutoScroll(true);
    setError(null);
    setTrackingState('not_live');
    setClearedCount(0);

    function connect() {
      if (cancelled) return;
      const token = getToken();
      const source = new EventSource(appEventStreamUrl(appName, token ?? undefined));
      eventSourceRef.current = source;

      source.onopen = () => {
        setError(null);
        setTrackingState('live');
      };

      source.onmessage = (message) => {
        const event = parseEvent(message.data);
        if (!event) return;
        handleIncomingEvent(event);
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
      ? 'Waiting for log events…'
      : trackingState === 'retrying'
        ? 'Retrying live stream…'
        : (error ??
          (logsQuery.error ? 'Unable to load historical logs.' : 'Live tracking is not active.'));

  return (
    <div className="panel log-panel">
      <div className="log-toolbar">
        <div className="log-status">
          <span className="log-status-dot" data-state={statusTone} />
          <div className="log-status-copy">
            <span className="log-status-label" data-state={trackingState}>
              {statusLabel}
            </span>
            <span className="log-status-meta">{entries.length} entries</span>
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
            <span className="log-source">[{entry.eventType}]</span>
            <span className="log-message">{entry.message}</span>
          </div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}

function parseEvent(data: string): EventRecord | null {
  try {
    return JSON.parse(data) as EventRecord;
  } catch {
    return null;
  }
}

function parsePayload(payload: string | null | undefined): Record<string, unknown> | null {
  if (!payload) return null;
  try {
    return JSON.parse(payload) as Record<string, unknown>;
  } catch {
    return null;
  }
}

function eventToEntry(event: EventRecord): AppLogEntry | null {
  const payload = parsePayload(event.payload);
  if (typeof payload?.line === 'string') {
    return {
      id: event.id,
      createdAt: event.created_at,
      eventType: event.event_type,
      message: payload.line,
    };
  }

  if (event.event_type.startsWith('deploy.') || event.event_type.startsWith('build.')) {
    return {
      id: event.id,
      createdAt: event.created_at,
      eventType: event.event_type,
      message: summariseEvent(event.event_type, payload),
    };
  }

  return null;
}

function isTerminalDeployEvent(eventType: string): boolean {
  return (
    eventType === 'deploy.live' || eventType === 'deploy.failed' || eventType === 'deploy.rollback'
  );
}

function summariseEvent(type: string, payload: Record<string, unknown> | null): string {
  if (typeof payload?.url === 'string') return `Live at ${payload.url}`;
  if (typeof payload?.error === 'string') return payload.error;
  if (typeof payload?.command === 'string') return `Release phase: ${payload.command}`;
  if (typeof payload?.builder === 'string') return `Builder ${payload.builder}`;
  return type.replaceAll('.', ' ');
}

function formatTimestamp(value: string): string {
  return new Date(value).toLocaleTimeString('en-US', {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}
