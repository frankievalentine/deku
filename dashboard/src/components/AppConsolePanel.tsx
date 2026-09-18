import { useEffect, useRef, useState } from 'react';
import { type ConsoleOutputFrame, streamConsoleCommand } from '../lib/api';
import SelectField from './SelectField';

interface AppConsolePanelProps {
  appName: string;
  locked: boolean;
  className?: string;
}

interface ConsoleLine extends ConsoleOutputFrame {
  id: number;
}

type ConsoleMode = 'run' | 'exec';

/** Splits a command line into arguments, honoring simple single/double quotes. */
function splitCommand(input: string): string[] {
  const args: string[] = [];
  let current = '';
  let quote: '"' | "'" | null = null;
  let started = false;

  for (const char of input.trim()) {
    if (quote) {
      if (char === quote) {
        quote = null;
      } else {
        current += char;
      }
      started = true;
      continue;
    }
    if (char === '"' || char === "'") {
      quote = char;
      started = true;
      continue;
    }
    if (/\s/.test(char)) {
      if (started) {
        args.push(current);
        current = '';
        started = false;
      }
      continue;
    }
    current += char;
    started = true;
  }
  if (started) args.push(current);
  return args;
}

export default function AppConsolePanel({ appName, locked, className }: AppConsolePanelProps) {
  const [mode, setMode] = useState<ConsoleMode>('run');
  const [command, setCommand] = useState('');
  const [lines, setLines] = useState<ConsoleLine[]>([]);
  const [exitCode, setExitCode] = useState<number | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<string[]>([]);
  const abortRef = useRef<(() => void) | null>(null);
  const outputRef = useRef<HTMLPreElement | null>(null);
  const nextIdRef = useRef(0);

  useEffect(() => {
    return () => abortRef.current?.();
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: scroll to the newest output whenever a line arrives.
  useEffect(() => {
    if (outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [lines]);

  function handleRun() {
    if (running) {
      abortRef.current?.();
      abortRef.current = null;
      setRunning(false);
      return;
    }

    const args = splitCommand(command);
    if (args.length === 0) {
      setError('Type a command to run, for example npm run migrate.');
      return;
    }

    setError(null);
    setExitCode(null);
    setRunning(true);
    setLines([]);
    setHistory((current) =>
      [command.trim(), ...current.filter((entry) => entry !== command.trim())].slice(0, 8)
    );

    abortRef.current = streamConsoleCommand(appName, mode, args, {
      onOutput: (frame) => {
        nextIdRef.current += 1;
        setLines((current) => [...current, { ...frame, id: nextIdRef.current }]);
      },
      onExit: (code) => {
        setExitCode(code);
        setRunning(false);
        abortRef.current = null;
      },
      onError: (message) => {
        setError(message);
        setRunning(false);
        abortRef.current = null;
      },
    });
  }

  return (
    <article className={`panel stack-md${className ? ` ${className}` : ''}`}>
      <div className="panel-heading">
        <div className="stack-sm panel-heading-copy">
          <p className="eyebrow">Console</p>
          <h2 className="section-title">Run a command</h2>
          <p className="page-copy">
            Run a one-off command in a fresh copy of the app image, or run it in the container that
            is already serving traffic. Output streams here and the command exits when it finishes.
          </p>
        </div>
        <span className="inventory-summary">
          {mode === 'run' ? 'Fresh container' : 'Live container'}
        </span>
      </div>

      <div className="panel-grid">
        <div className="form-group">
          <label className="form-label" htmlFor="console-mode">
            Run in
          </label>
          <SelectField
            id="console-mode"
            value={mode}
            onChange={(next) => setMode(next as ConsoleMode)}
            disabled={running}
            options={[
              { value: 'run', label: 'Fresh container (safe)' },
              { value: 'exec', label: 'Live container' },
            ]}
          />
        </div>
        <div className="form-group">
          <label className="form-label" htmlFor="console-command">
            Command
          </label>
          <input
            id="console-command"
            className="input font-mono"
            placeholder="npm run migrate"
            value={command}
            onChange={(event) => setCommand(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                handleRun();
              }
            }}
            disabled={running}
          />
        </div>
      </div>

      {mode === 'exec' ? (
        <p className="callout callout-warning">
          This runs inside the live container. Changes are lost on the next deploy and may affect
          visitors while it runs.
        </p>
      ) : null}

      {error ? <p className="callout callout-danger">{error}</p> : null}
      {exitCode !== null ? (
        <p className={`callout ${exitCode === 0 ? 'callout-success' : 'callout-danger'}`}>
          Command finished with exit code {exitCode}.
        </p>
      ) : null}

      <div className="form-actions">
        <button
          className={`btn ${running ? 'btn-secondary' : 'btn-primary'}`}
          type="button"
          onClick={handleRun}
          disabled={locked || (!running && command.trim().length === 0)}
        >
          {running ? 'Stop' : 'Run command'}
        </button>
        <button
          className="btn btn-ghost"
          type="button"
          onClick={() => {
            setLines([]);
            setExitCode(null);
            setError(null);
          }}
          disabled={running || (lines.length === 0 && exitCode === null)}
        >
          Clear output
        </button>
      </div>

      <pre ref={outputRef} className="console-output" aria-live="polite">
        {lines.length === 0 ? (
          <span className="text-muted">Output appears here.</span>
        ) : (
          lines.map((line) => (
            <span
              key={line.id}
              className={
                line.stream === 'stderr' ? 'console-line console-line-error' : 'console-line'
              }
            >
              {line.line}
            </span>
          ))
        )}
      </pre>

      {history.length > 0 ? (
        <div className="stack-sm">
          <h3 className="deploy-title">Recent commands</h3>
          <div className="button-row">
            {history.map((entry) => (
              <button
                key={entry}
                className="btn btn-ghost btn-sm"
                type="button"
                onClick={() => setCommand(entry)}
                disabled={running}
              >
                {entry}
              </button>
            ))}
          </div>
        </div>
      ) : null}
    </article>
  );
}
