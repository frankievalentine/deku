import { ArrowDown } from 'lucide-react';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { clock } from '../lib/format';
import type { LogLine } from '../lib/types';

/**
 * A terminal-style log pane. It follows new lines while the reader is at the bottom and stops
 * following as soon as they scroll up, with a button to jump back.
 */
export function LogView({
  lines,
  height = 'h-[320px]',
  highlight,
  showProcess = true,
  emptyText = 'No output yet.',
}: {
  lines: LogLine[];
  height?: string;
  highlight?: string;
  showProcess?: boolean;
  emptyText?: string;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const [following, setFollowing] = useState(true);
  const seen = useRef(new Set<string>());
  const firstPaint = useRef(true);

  // New lines are the trigger for scrolling to the bottom, even though the body only reads the ref.
  // biome-ignore lint/correctness/useExhaustiveDependencies: lines is the trigger, not an input
  useLayoutEffect(() => {
    const element = scroller.current;
    if (element && following) element.scrollTop = element.scrollHeight;
  }, [lines, following]);

  useEffect(() => {
    // Lines present on first paint do not animate in; only lines that arrive afterwards do.
    if (firstPaint.current) {
      for (const line of lines) seen.current.add(line.id);
      firstPaint.current = false;
    }
  }, [lines]);

  const onScroll = () => {
    const element = scroller.current;
    if (!element) return;
    const atBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 24;
    setFollowing(atBottom);
  };

  const needle = highlight?.toLowerCase();

  return (
    <div className="relative">
      <div
        ref={scroller}
        onScroll={onScroll}
        role="log"
        aria-live="off"
        // A scrollable region has to be reachable by keyboard so it can be scrolled without a mouse.
        // biome-ignore lint/a11y/noNoninteractiveTabindex: scrollable regions must be focusable
        tabIndex={0}
        className={`rounded-ui overflow-auto bg-term py-2 font-mono text-[12px] leading-[1.65] text-term-ink ${height}`}
      >
        {lines.length === 0 ? <p className="px-3.5 py-2 text-term-ink/50">{emptyText}</p> : null}
        {lines.map((line) => {
          const fresh = !seen.current.has(line.id);
          if (fresh) seen.current.add(line.id);
          const marked = needle ? line.message.toLowerCase().includes(needle) : false;
          return (
            <div
              key={line.id}
              className={`grid grid-cols-[auto_1fr] gap-x-3 px-3.5 hover:bg-white/[0.04] sm:grid-cols-[auto_auto_1fr] ${
                fresh && !firstPaint.current ? 'line-enter' : ''
              } ${marked ? 'bg-warn/15' : ''}`}
            >
              <span className="tabular text-term-ink/40 select-none">{clock(line.created_at)}</span>
              {showProcess ? (
                <span className="hidden w-[76px] truncate text-term-ink/55 select-none sm:block">
                  {line.process}
                </span>
              ) : null}
              <span
                className={`break-words whitespace-pre-wrap ${
                  line.stream === 'stderr'
                    ? 'text-[oklch(0.78_0.13_40)]'
                    : line.message.startsWith('==>')
                      ? 'font-medium text-term-ink'
                      : 'text-term-ink/85'
                } ${showProcess ? '' : 'col-span-1'}`}
              >
                {line.message}
              </span>
            </div>
          );
        })}
      </div>
      {!following ? (
        <button
          type="button"
          onClick={() => setFollowing(true)}
          className="rounded-ui absolute right-3 bottom-3 inline-flex items-center gap-1.5 border border-white/15 bg-term px-2.5 py-1 font-sans text-[12px] text-term-ink shadow-lg"
        >
          <ArrowDown className="size-3.5" />
          Follow
        </button>
      ) : null}
    </div>
  );
}
