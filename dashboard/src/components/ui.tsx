import { Check, Copy, Terminal } from 'lucide-react';
import {
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  useState,
  useSyncExternalStore,
} from 'react';
import { isInProgress, STATUS_LABEL } from '../lib/format';
import type { AppStatus, CheckStatus, DeployStatus } from '../lib/types';

export type Tone = 'ok' | 'warn' | 'bad' | 'busy' | 'idle' | 'accent';

const DOT_COLOR: Record<Tone, string> = {
  ok: 'bg-ok text-ok',
  warn: 'bg-warn text-warn',
  bad: 'bg-bad text-bad',
  busy: 'bg-busy text-busy',
  idle: 'bg-ink-3/60 text-ink-3',
  accent: 'bg-accent text-accent',
};

export function Dot({
  tone,
  pulse = false,
  className = '',
}: {
  tone: Tone;
  pulse?: boolean;
  className?: string;
}) {
  return (
    <span
      aria-hidden="true"
      className={`inline-block size-[7px] shrink-0 rounded-full ${DOT_COLOR[tone]} ${pulse ? 'dot-pulse' : ''} ${className}`}
    />
  );
}

export function deployTone(status: DeployStatus): Tone {
  if (status === 'live') return 'ok';
  if (status === 'failed') return 'bad';
  if (status === 'rolled_back') return 'idle';
  return 'busy';
}

export function DeployState({
  status,
  current = true,
}: {
  status: DeployStatus;
  current?: boolean;
}) {
  const superseded = status === 'live' && !current;
  return (
    <span className="inline-flex items-center gap-1.5 whitespace-nowrap">
      <Dot tone={superseded ? 'idle' : deployTone(status)} pulse={isInProgress(status)} />
      <span className={superseded ? 'text-ink-3' : status === 'failed' ? 'text-bad' : 'text-ink'}>
        {superseded ? 'Superseded' : STATUS_LABEL[status]}
      </span>
    </span>
  );
}

const APP_STATE: Record<AppStatus, [Tone, string]> = {
  deployed: ['ok', 'Running'],
  created: ['idle', 'Not deployed'],
  stopped: ['idle', 'Stopped'],
  error: ['bad', 'Error'],
};

export function AppState({
  status,
  deploying,
  maintenance,
}: {
  status: AppStatus;
  deploying?: boolean;
  maintenance?: boolean;
}) {
  const [tone, label] = deploying
    ? (['busy', 'Deploying'] as [Tone, string])
    : maintenance
      ? (['warn', 'Maintenance'] as [Tone, string])
      : APP_STATE[status];
  return (
    <span className="inline-flex items-center gap-1.5 whitespace-nowrap">
      <Dot tone={tone} pulse={deploying} />
      <span>{label}</span>
    </span>
  );
}

export const CHECK_TONE: Record<CheckStatus, Tone> = { pass: 'ok', warn: 'warn', fail: 'bad' };

type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger';

const BUTTON: Record<ButtonVariant, string> = {
  primary: 'bg-accent text-on-accent hover:brightness-105 border border-transparent font-medium',
  secondary: 'bg-surface text-ink border border-line-2 hover:bg-sunk',
  ghost: 'text-ink-2 hover:text-ink hover:bg-sunk border border-transparent',
  danger: 'bg-surface text-bad border border-line-2 hover:border-bad',
};

export function Button({
  variant = 'secondary',
  size = 'md',
  className = '',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant; size?: 'sm' | 'md' }) {
  const sizing = size === 'sm' ? 'h-7 px-2.5 text-[12.5px] gap-1.5' : 'h-8 px-3 text-[13px] gap-2';
  return (
    <button
      type="button"
      className={`rounded-ui inline-flex shrink-0 items-center justify-center whitespace-nowrap transition-colors disabled:pointer-events-none disabled:opacity-50 ${sizing} ${BUTTON[variant]} ${className}`}
      {...props}
    />
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded-ui inline-flex h-[18px] min-w-[18px] items-center justify-center border border-line px-1 font-mono text-[10.5px] text-ink-3">
      {children}
    </kbd>
  );
}

/** The CLI command that does the same thing as the control next to it. */
export function Cli({ command, className = '' }: { command: string; className?: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      title="Copy the CLI command"
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(command);
          setCopied(true);
          window.setTimeout(() => setCopied(false), 1500);
        } catch {
          setCopied(false);
        }
      }}
      className={`group rounded-ui inline-flex max-w-full items-center gap-2 border border-dashed border-line-2 px-2 py-1 text-left font-mono text-[11.5px] text-ink-2 hover:border-ink-3 hover:text-ink ${className}`}
    >
      <Terminal aria-hidden="true" className="size-3 shrink-0 text-ink-3" />
      <span className="truncate">{command}</span>
      {copied ? (
        <Check aria-hidden="true" className="size-3 shrink-0 text-ok" />
      ) : (
        <Copy
          aria-hidden="true"
          className="size-3 shrink-0 text-ink-3 opacity-0 group-hover:opacity-100"
        />
      )}
      <span className="sr-only" aria-live="polite">
        {copied ? 'Copied' : ''}
      </span>
    </button>
  );
}

export function Section({
  id,
  title,
  meta,
  actions,
  children,
  className = '',
}: {
  id?: string;
  title: string;
  meta?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section id={id} className={`min-w-0 scroll-mt-20 ${className}`}>
      <header className="mb-2 flex min-h-8 items-center gap-3 border-b border-line pb-2">
        <h2 className="section-title text-[13px] font-semibold text-ink">{title}</h2>
        {meta ? <span className="font-mono text-[11.5px] text-ink-3">{meta}</span> : null}
        {actions ? <div className="ml-auto flex items-center gap-2">{actions}</div> : null}
      </header>
      {children}
    </section>
  );
}

export function Empty({
  title,
  children,
  command,
}: {
  title: string;
  children?: ReactNode;
  command?: string;
}) {
  return (
    <div className="rounded-ui flex flex-col items-start gap-2 border border-dashed border-line-2 px-4 py-5">
      <p className="font-medium text-ink">{title}</p>
      {children ? <p className="max-w-prose text-ink-2">{children}</p> : null}
      {command ? <Cli command={command} /> : null}
    </div>
  );
}

/** The last deploys as a strip of ticks, newest on the right. */
export function DeployStrip({ statuses }: { statuses: DeployStatus[] }) {
  const ordered = statuses.slice(0, 12).reverse();
  return (
    <span
      className="inline-flex h-4 items-end gap-[3px]"
      role="img"
      aria-label={`${ordered.filter((s) => s === 'failed').length} failed of the last ${ordered.length} deploys`}
    >
      {ordered.map((status, index) => (
        <span
          // The strip is positional; the index is the identity.
          // biome-ignore lint/suspicious/noArrayIndexKey: positions, not records
          key={index}
          className={`w-[3px] rounded-[1px] ${
            status === 'failed'
              ? 'h-4 bg-bad'
              : isInProgress(status)
                ? 'h-4 bg-busy dot-pulse'
                : 'h-2.5 bg-ink-3/45'
          }`}
        />
      ))}
    </span>
  );
}

export function Meter({ value, max, tone = 'idle' }: { value: number; max: number; tone?: Tone }) {
  const ratio = Math.min(1, value / max);
  const color = tone === 'warn' ? 'bg-warn' : tone === 'bad' ? 'bg-bad' : 'bg-ink-2';
  return (
    <span className="block h-1 w-full overflow-hidden rounded-full bg-line">
      <span className={`block h-full rounded-full ${color}`} style={{ width: `${ratio * 100}%` }} />
    </span>
  );
}

// ── Toasts ────────────────────────────────────────────────────────────────────

interface Toast {
  id: number;
  tone: Tone;
  text: string;
}

let toasts: Toast[] = [];
let nextToast = 1;
const toastListeners = new Set<() => void>();

export function toast(text: string, tone: Tone = 'ok'): void {
  const entry = { id: nextToast++, tone, text };
  toasts = [...toasts, entry].slice(-3);
  for (const listener of toastListeners) listener();
  window.setTimeout(() => {
    toasts = toasts.filter((t) => t.id !== entry.id);
    for (const listener of toastListeners) listener();
  }, 3600);
}

export function Toaster() {
  const list = useSyncExternalStore(
    (listener) => {
      toastListeners.add(listener);
      return () => toastListeners.delete(listener);
    },
    () => toasts
  );
  return (
    <div
      className="pointer-events-none fixed right-4 bottom-4 z-50 flex flex-col gap-2"
      aria-live="polite"
    >
      {list.map((entry) => (
        <div
          key={entry.id}
          className="line-enter rounded-ui flex items-center gap-2 border border-line-2 bg-surface px-3 py-2 text-[13px] shadow-lg shadow-black/10"
        >
          <Dot tone={entry.tone} />
          {entry.text}
        </div>
      ))}
    </div>
  );
}

// ── Forms ─────────────────────────────────────────────────────────────────────

const FIELD =
  'rounded-ui h-8 border border-line-2 bg-surface px-2.5 text-[13px] text-ink outline-none placeholder:text-ink-3 focus:border-ink-3 disabled:opacity-60';

export function Input({
  className = '',
  mono = false,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { mono?: boolean }) {
  return (
    <input
      spellCheck={false}
      className={`${FIELD} ${mono ? 'font-mono text-[12.5px]' : ''} ${className}`}
      {...props}
    />
  );
}

export function Select({
  className = '',
  children,
  ...props
}: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select className={`${FIELD} pr-7 ${className}`} {...props}>
      {children}
    </select>
  );
}

/** A labeled control. The label stays visible; the hint says what a good value looks like. */
export function Field({
  label,
  hint,
  error,
  className = '',
  children,
}: {
  label: string;
  hint?: ReactNode;
  error?: string | null;
  className?: string;
  children: ReactNode;
}) {
  return (
    // The control arrives as children, so the wrapping label is associated with it implicitly.
    // biome-ignore lint/a11y/noLabelWithoutControl: the control is passed in as children
    <label className={`flex min-w-0 flex-col gap-1.5 ${className}`}>
      <span className="text-[12.5px] leading-[18px] font-medium text-ink">{label}</span>
      {children}
      {error ? (
        <span className="text-[12px] text-bad">{error}</span>
      ) : hint ? (
        <span className="text-[12px] text-ink-3">{hint}</span>
      ) : null}
    </label>
  );
}

/**
 * A remove button that asks once. The first press turns it into a confirmation naming the
 * consequence; it resets if not confirmed within a few seconds.
 */
export function ConfirmButton({
  label,
  confirmLabel,
  onConfirm,
  disabled,
  size = 'sm',
  variant = 'ghost',
  className = '',
}: {
  label: string;
  confirmLabel: string;
  onConfirm: () => void;
  disabled?: boolean;
  size?: 'sm' | 'md';
  variant?: 'ghost' | 'secondary';
  className?: string;
}) {
  const [armed, setArmed] = useState(false);
  return (
    <Button
      size={size}
      variant={armed ? 'danger' : variant}
      className={className}
      disabled={disabled}
      onClick={() => {
        if (armed) {
          setArmed(false);
          onConfirm();
          return;
        }
        setArmed(true);
        window.setTimeout(() => setArmed(false), 3500);
      }}
    >
      {armed ? confirmLabel : label}
    </Button>
  );
}

/** One row of a settings list: content on the left, actions on the right. */
export function Row({ children, actions }: { children: ReactNode; actions?: ReactNode }) {
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-1.5 border-b border-line py-2.5 last:border-b-0">
      <div className="min-w-0 flex-1">{children}</div>
      {actions ? <div className="flex shrink-0 items-center gap-1">{actions}</div> : null}
    </li>
  );
}

/** A jump list for long settings pages; stays in view beside the sections on wide screens. */
export function SectionNav({ items }: { items: Array<[string, string]> }) {
  return (
    <nav aria-label="On this page" className="hidden xl:block">
      <ul className="sticky top-20 flex flex-col gap-px border-l border-line">
        {items.map(([id, label]) => (
          <li key={id}>
            <a
              href={`#${id}`}
              className="-ml-px block border-l border-transparent py-1 pl-3 text-[12.5px] text-ink-3 hover:border-ink-3 hover:text-ink"
            >
              {label}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}
