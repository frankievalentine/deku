// Requests to the daemon. The browser authenticates with an HttpOnly session
// cookie the daemon sets at sign-in, so no token is ever held in page memory or
// storage. Writes carry the CSRF header the daemon requires for cookie
// sessions; a 401 anywhere means the session ended, and the shell shows the
// sign-in screen.

export const SIGNED_OUT_EVENT = 'deku:signed-out';
const CSRF_HEADER = 'X-Deku-CSRF';

export class ApiError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.status = status;
  }
}

function isWrite(method: string | undefined): boolean {
  return !['GET', 'HEAD', 'OPTIONS'].includes((method ?? 'GET').toUpperCase());
}

async function errorMessage(response: Response, fallback: string): Promise<string> {
  const text = await response.text().catch(() => '');
  try {
    const parsed: unknown = JSON.parse(text);
    if (parsed && typeof parsed === 'object' && 'error' in parsed) {
      const error = (parsed as { error: unknown }).error;
      if (typeof error === 'string') return error;
    }
  } catch {
    // Not JSON: fall through to the plain text or the fallback.
  }
  return text.trim() || fallback;
}

function headersFor(init: RequestInit): Headers {
  const headers = new Headers(init.headers ?? {});
  const isForm = typeof FormData !== 'undefined' && init.body instanceof FormData;
  if (init.body && !isForm && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  if (isWrite(init.method)) headers.set(CSRF_HEADER, '1');
  return headers;
}

/** Call the daemon and return its JSON body; a 204 returns `undefined`. */
export async function apiFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: headersFor(init),
    credentials: 'same-origin',
  });
  if (response.status === 401) {
    window.dispatchEvent(new Event(SIGNED_OUT_EVENT));
    throw new ApiError('Sign in to continue.', 401);
  }
  if (!response.ok) {
    throw new ApiError(
      await errorMessage(response, `The daemon answered ${response.status}.`),
      response.status
    );
  }
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

/** Fetch a plain-text body, such as Prometheus metrics. */
export async function apiText(path: string): Promise<string> {
  const response = await fetch(path, { credentials: 'same-origin' });
  if (response.status === 401) {
    window.dispatchEvent(new Event(SIGNED_OUT_EVENT));
    throw new ApiError('Sign in to continue.', 401);
  }
  if (!response.ok) throw new ApiError(`The daemon answered ${response.status}.`, response.status);
  return response.text();
}

/**
 * Pass each server-sent event's `data`, and its `event` name when it has one,
 * to `onData` until the body ends. Comment frames, such as keep-alives, carry
 * no data and are skipped.
 */
async function readSse(
  body: ReadableStream<Uint8Array>,
  onData: (data: string, event: string | undefined) => void
): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  for (;;) {
    const { value, done } = await reader.read();
    if (done) return;
    buffer += decoder.decode(value, { stream: true });
    let boundary = buffer.indexOf('\n\n');
    while (boundary !== -1) {
      const lines = buffer.slice(0, boundary).split('\n');
      buffer = buffer.slice(boundary + 2);
      const data = lines
        .filter((line) => line.startsWith('data:'))
        .map((line) => line.slice(5).trimStart())
        .join('\n');
      const event = lines
        .find((line) => line.startsWith('event:'))
        ?.slice(6)
        .trim();
      if (data) onData(data, event);
      boundary = buffer.indexOf('\n\n');
    }
  }
}

/**
 * Follow a GET event stream, reconnecting after a drop, until the returned
 * function is called.
 */
export function followStream(
  path: string,
  onData: (data: string, event: string | undefined) => void,
  onState?: (connected: boolean) => void
): () => void {
  let stopped = false;
  let controller = new AbortController();
  const RECONNECT_MS = 2500;

  const connect = async () => {
    while (!stopped) {
      controller = new AbortController();
      try {
        const response = await fetch(path, {
          headers: { Accept: 'text/event-stream' },
          credentials: 'same-origin',
          signal: controller.signal,
        });
        if (response.status === 401) {
          window.dispatchEvent(new Event(SIGNED_OUT_EVENT));
          return;
        }
        if (response.ok && response.body) {
          onState?.(true);
          await readSse(response.body, onData);
        }
      } catch {
        if (stopped) return;
      }
      onState?.(false);
      if (!stopped) await new Promise((resolve) => window.setTimeout(resolve, RECONNECT_MS));
    }
  };
  void connect();
  return () => {
    stopped = true;
    controller.abort();
  };
}

/**
 * Run a command with `run` or `exec`, collecting its output until it exits.
 * The daemon streams `{stream, line}` frames and ends with `{exit_code}`.
 */
export async function runConsole(
  path: string,
  command: string[]
): Promise<{ exitCode: number; output: string[] }> {
  const response = await fetch(path, {
    method: 'POST',
    headers: headersFor({ method: 'POST', body: '{}' }),
    credentials: 'same-origin',
    body: JSON.stringify({ command }),
  });
  if (response.status === 401) {
    window.dispatchEvent(new Event(SIGNED_OUT_EVENT));
    throw new ApiError('Sign in to continue.', 401);
  }
  if (!response.ok || !response.body) {
    throw new ApiError(
      await errorMessage(response, 'The command could not start.'),
      response.status
    );
  }
  const output: string[] = [];
  let exitCode = 0;
  await readSse(response.body, (data) => {
    try {
      const frame = JSON.parse(data) as { line?: string; exit_code?: number };
      if (typeof frame.line === 'string') output.push(frame.line);
      if (typeof frame.exit_code === 'number') exitCode = frame.exit_code;
    } catch {
      // A malformed frame is skipped; the rest of the output still arrives.
    }
  });
  return { exitCode, output };
}
