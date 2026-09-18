import { type SubmitEvent, useCallback, useEffect, useState } from 'react';
import {
  type BackupSchedule,
  deleteServiceBackupSchedule,
  fetchServiceBackupSchedule,
  setServiceBackupSchedule,
} from '../lib/api';
import ConfirmModal from './ConfirmModal';
import SelectField from './SelectField';
import Spinner from './Spinner';

interface ServiceBackupScheduleProps {
  name: string;
  locked: boolean;
}

const INTERVALS = [
  { value: '6', label: 'Every 6 hours' },
  { value: '12', label: 'Every 12 hours' },
  { value: '24', label: 'Every day' },
  { value: '168', label: 'Every week' },
];

const RETENTIONS = [
  { value: '3', label: 'Keep 3 backups' },
  { value: '7', label: 'Keep 7 backups' },
  { value: '14', label: 'Keep 14 backups' },
  { value: '30', label: 'Keep 30 backups' },
];

function formatDate(value: string | null | undefined): string {
  if (!value) return 'Never';
  return new Date(value).toLocaleString();
}

export default function ServiceBackupSchedule({ name, locked }: ServiceBackupScheduleProps) {
  const [schedule, setSchedule] = useState<BackupSchedule | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [interval, setIntervalValue] = useState('24');
  const [retention, setRetention] = useState('7');
  const [confirmStop, setConfirmStop] = useState(false);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const next = await fetchServiceBackupSchedule(name);
      setSchedule(next);
      if (next.interval_hours) setIntervalValue(String(next.interval_hours));
      if (next.retention) setRetention(String(next.retention));
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : 'Unable to load the backup schedule.'
      );
    } finally {
      setLoading(false);
    }
  }, [name]);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleSave(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (locked) return;

    try {
      setBusy('save');
      setError(null);
      setNotice(null);
      const next = await setServiceBackupSchedule(name, {
        interval_hours: Number(interval),
        retention: Number(retention),
      });
      setSchedule(next);
      setNotice('Automatic backups are on.');
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to save the schedule.');
    } finally {
      setBusy(null);
    }
  }

  async function handleStop() {
    try {
      setBusy('stop');
      setError(null);
      setNotice(null);
      await deleteServiceBackupSchedule(name);
      setConfirmStop(false);
      await load();
      setNotice('Automatic backups are off. Existing backups are kept.');
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to stop the schedule.');
    } finally {
      setBusy(null);
    }
  }

  const enabled = schedule?.enabled ?? false;

  return (
    <>
      <div className="stack-md">
        <div className="cluster justify-between align-center">
          <h3 className="deploy-title">Automatic backups</h3>
          <span className="inventory-summary">{enabled ? 'On' : 'Off'}</span>
        </div>

        {notice ? <p className="callout callout-success">{notice}</p> : null}
        {error ? <p className="callout callout-danger">{error}</p> : null}

        {loading ? (
          <div className="loading-state">
            <Spinner />
            <span>Loading backup schedule…</span>
          </div>
        ) : (
          <>
            {enabled ? (
              <dl className="data-grid">
                <div>
                  <dt>Runs</dt>
                  <dd>Every {schedule?.interval_hours} hours</dd>
                </div>
                <div>
                  <dt>Keeps</dt>
                  <dd>{schedule?.retention} backups</dd>
                </div>
                <div>
                  <dt>Last run</dt>
                  <dd>{formatDate(schedule?.last_run_at)}</dd>
                </div>
                <div>
                  <dt>Next run</dt>
                  <dd>{formatDate(schedule?.next_run_at)}</dd>
                </div>
              </dl>
            ) : (
              <p className="text-muted">
                No automatic backups. Turn them on so a recent copy is always available.
              </p>
            )}

            <form onSubmit={handleSave} className="stack-md" noValidate>
              <div className="panel-grid">
                <div className="form-group">
                  <label className="form-label" htmlFor="backup-interval">
                    How often
                  </label>
                  <SelectField
                    id="backup-interval"
                    value={interval}
                    onChange={setIntervalValue}
                    disabled={locked || busy !== null}
                    options={INTERVALS}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label" htmlFor="backup-retention">
                    How many to keep
                  </label>
                  <SelectField
                    id="backup-retention"
                    value={retention}
                    onChange={setRetention}
                    disabled={locked || busy !== null}
                    options={RETENTIONS}
                  />
                </div>
              </div>
              <div className="form-actions">
                <button
                  className="btn btn-primary"
                  type="submit"
                  disabled={locked || busy !== null}
                >
                  {busy === 'save' ? <span className="loading-spinner" /> : null}
                  <span>{enabled ? 'Update schedule' : 'Turn on automatic backups'}</span>
                </button>
                {enabled ? (
                  <button
                    className="btn btn-danger"
                    type="button"
                    onClick={() => setConfirmStop(true)}
                    disabled={locked || busy !== null}
                  >
                    Turn off
                  </button>
                ) : null}
              </div>
            </form>
          </>
        )}
      </div>

      <ConfirmModal
        open={confirmStop}
        title="Turn off automatic backups?"
        description={`${name} will stop being backed up on a schedule. Backups already taken are kept.`}
        confirmLabel="Turn off backups"
        busy={busy === 'stop'}
        onClose={() => {
          if (busy !== 'stop') setConfirmStop(false);
        }}
        onConfirm={() => {
          void handleStop();
        }}
      />
    </>
  );
}
