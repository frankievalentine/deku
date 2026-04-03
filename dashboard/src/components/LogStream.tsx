import { useEffect, useRef, useState } from 'react';
import { type EventRecord, appEventStreamUrl, fetchLogs, getToken } from '../lib/api';

interface LogStreamProps {
  appName: string;
}

interface LogEntry {
  id: string;
  createdAt: string | null;
  message: string;
  eventType: string;
}

export default function LogStream({ appName }: LogStreamProps) {
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [connected, setConnected] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectRef = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadRecentLogs() {
      try {
        const lines = await fetchLogs(appName, 120);
        if (cancelled) return;
        setEntries(
          lines.map((line, index) => ({
            id: `tail-${index}-${line}`,
            createdAt: null,
            eventType: 'log.tail',
            message: line,
          }))
        );
      } catch {
        if (!cancelled) {
          setError('Unable to load historical logs.');
        }
      }
    }

    function connect() {
      if (cancelled) return;
      const token = getToken();
      const source = new EventSource(appEventStreamUrl(appName, token ?? undefined));
      eventSourceRef.current = source;
      setError(null);

      source.onopen = () => {
        setConnected(true);
      };

      source.onmessage = (message) => {
        const event = parseEvent(message.data);
        if (!event) return;
        const nextEntry = eventToEntry(event);
        if (!nextEntry) return;

        setEntries((current) => {
          const capped = current.length >= 500 ? current.slice(-499) : current;
          return [...capped, nextEntry];
        });
      };

      source.onerror = () => {
        setConnected(false);
        setError('Stream interrupted. Retrying…');
        source.close();
        if (reconnectRef.current !== null) {
          window.clearTimeout(reconnectRef.current);
        }
        reconnectRef.current = window.setTimeout(connect, 2500);
      };
    }

    void loadRecentLogs();
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
    if (autoScroll && bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: 'auto' });
    }
  }, [autoScroll]);

  function handleScroll() {
    if (!containerRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = containerRef.current;
    const nearBottom = scrollHeight - scrollTop - clientHeight < 60;
    setAutoScroll(nearBottom);
  }

  return (
    <div className="panel log-panel">
      <div className="log-toolbar">
        <div className="cluster">
          <span className="log-status-dot" data-connected={connected} />
          <span className="text-muted">{connected ? 'Live' : (error ?? 'Connecting…')}</span>
          <span className="font-mono text-muted">{entries.length} entries</span>
        </div>
        <div className="cluster">
          <button
            className={`btn btn-ghost btn-sm ${autoScroll ? 'is-active' : ''}`}
            onClick={() => setAutoScroll((current) => !current)}
            type="button"
          >
            {autoScroll ? 'Auto-scroll on' : 'Auto-scroll off'}
          </button>
          <button className="btn btn-ghost btn-sm" onClick={() => setEntries([])} type="button">
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
        {entries.length === 0 && (
          <div className="log-empty">
            {connected ? 'Waiting for log events…' : 'Connecting to log stream…'}
          </div>
        )}
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

function eventToEntry(event: EventRecord): LogEntry | null {
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
