import { type SubmitEvent, useCallback, useEffect, useState } from 'react';
import { type AppLimit, fetchAppLimits, setAppLimit } from '../lib/api';
import Spinner from './Spinner';

interface AppLimitsPanelProps {
  appName: string;
  locked: boolean;
}

export default function AppLimitsPanel({ appName, locked }: AppLimitsPanelProps) {
  const [limits, setLimits] = useState<AppLimit[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [processType, setProcessType] = useState('web');
  const [memory, setMemory] = useState('');
  const [cpu, setCpu] = useState('');

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      setLimits(await fetchAppLimits(appName));
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to load resource limits.');
    } finally {
      setLoading(false);
    }
  }, [appName]);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleSave(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (locked) return;

    const type = processType.trim() || 'web';
    const nextMemory = memory.trim();
    const nextCpu = cpu.trim();

    if (!nextMemory && !nextCpu) {
      setError('Set a memory limit, a CPU limit, or both.');
      return;
    }

    try {
      setBusy('save');
      setError(null);
      setNotice(null);
      await setAppLimit(appName, {
        process_type: type,
        memory: nextMemory || null,
        cpu: nextCpu || null,
      });
      await load();
      setNotice(`Limits saved for ${type}. They apply on the next deploy.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to save the limit.');
    } finally {
      setBusy(null);
    }
  }

  return (
    <article className="panel stack-md">
      <div className="panel-heading">
        <div className="stack-sm panel-heading-copy">
          <p className="eyebrow">Resources</p>
          <h2 className="section-title">Limits</h2>
          <p className="page-copy">
            Cap memory and CPU per process. Leave a field blank for no limit. Changes apply on the
            next deploy.
          </p>
        </div>
        <span className="inventory-summary">
          {limits.length} limit{limits.length === 1 ? '' : 's'}
        </span>
      </div>

      {notice ? <p className="callout callout-success">{notice}</p> : null}
      {error ? <p className="callout callout-danger">{error}</p> : null}

      <form onSubmit={handleSave} className="stack-md" noValidate>
        <div className="panel-grid">
          <div className="form-group">
            <label className="form-label" htmlFor="limit-process-type">
              Process
            </label>
            <input
              id="limit-process-type"
              className="input"
              placeholder="web"
              value={processType}
              onChange={(event) => setProcessType(event.target.value)}
              disabled={locked || busy !== null}
            />
          </div>
          <div className="form-group">
            <label className="form-label" htmlFor="limit-memory">
              Memory
            </label>
            <input
              id="limit-memory"
              className="input"
              placeholder="512m"
              value={memory}
              onChange={(event) => setMemory(event.target.value)}
              disabled={locked || busy !== null}
            />
          </div>
          <div className="form-group">
            <label className="form-label" htmlFor="limit-cpu">
              CPU
            </label>
            <input
              id="limit-cpu"
              className="input"
              placeholder="0.5"
              value={cpu}
              onChange={(event) => setCpu(event.target.value)}
              disabled={locked || busy !== null}
            />
          </div>
        </div>
        <div className="form-actions">
          <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
            {busy === 'save' ? <span className="loading-spinner" /> : null}
            <span>Save limit</span>
          </button>
        </div>
      </form>

      {loading ? (
        <div className="loading-state">
          <Spinner />
          <span>Loading limits…</span>
        </div>
      ) : limits.length === 0 ? (
        <p className="text-muted">No limits set. The app can use the host's resources freely.</p>
      ) : (
        <dl className="data-grid">
          {limits.map((limit) => (
            <div key={limit.process_type ?? 'default'}>
              <dt>{limit.process_type ?? 'All processes'}</dt>
              <dd className="font-mono">
                {[limit.memory ? `${limit.memory}` : null, limit.cpu ? `${limit.cpu} CPU` : null]
                  .filter(Boolean)
                  .join(' - ') || 'No limit'}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </article>
  );
}
