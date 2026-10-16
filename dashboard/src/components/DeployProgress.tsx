import { Check, X } from 'lucide-react';
import { duration, isInProgress, STATUS_LABEL } from '../lib/format';
import type { Deployment, DeployStep } from '../lib/types';

export const STEPS: Array<{ id: DeployStep; label: string }> = [
  { id: 'build', label: 'Build' },
  { id: 'release', label: 'Release' },
  { id: 'health_check', label: 'Health check' },
  { id: 'live', label: 'Live' },
];

type StepState = 'done' | 'running' | 'failed' | 'skipped' | 'waiting';

export function stepState(deployment: Deployment, step: DeployStep): StepState {
  const timing = deployment.steps[step];
  if (deployment.failed_step === step) return 'failed';
  if (!timing) {
    if (isInProgress(deployment.status)) return 'waiting';
    // A deploy from before its events were kept has no step history; a live
    // one passed every step. Otherwise a finished deploy skipped this step, as
    // an image deploy skips the build or an app without a release command
    // skips the release.
    const noHistory = Object.keys(deployment.steps).length === 0;
    return noHistory && deployment.status === 'live' ? 'done' : 'skipped';
  }
  if (timing.finished_at || (step === 'live' && deployment.status === 'live')) return 'done';
  return isInProgress(deployment.status) ? 'running' : 'done';
}

function stepDuration(deployment: Deployment, step: DeployStep, now: number): string | null {
  const timing = deployment.steps[step];
  if (!timing) return null;
  if (step === 'live') return null;
  const end = timing.finished_at ? new Date(timing.finished_at).getTime() : now;
  return duration(end - new Date(timing.started_at).getTime());
}

const SEGMENT: Record<StepState, string> = {
  done: 'bg-ok',
  running: 'bg-busy',
  failed: 'bg-bad',
  skipped: 'bg-line',
  waiting: 'bg-line',
};

/** A deploy's position in build, release, health check, live. */
export function DeployProgress({
  deployment,
  now,
  compact = false,
  current = true,
}: {
  deployment: Deployment;
  now: number;
  compact?: boolean;
  /** Whether this deploy is the one serving now, rather than an earlier release. */
  current?: boolean;
}) {
  const elapsed = duration(now - new Date(deployment.created_at).getTime());

  if (compact) {
    return (
      <span className="block min-w-0">
        <span className="flex items-center gap-2 text-[12.5px]">
          <span className="font-medium text-busy">{STATUS_LABEL[deployment.status]}</span>
          <span className="tabular font-mono text-ink-3">{elapsed}</span>
        </span>
        <span className="mt-1.5 flex max-w-[180px] gap-[3px]" aria-hidden="true">
          {STEPS.map((step) => {
            const state = stepState(deployment, step.id);
            return (
              <span
                key={step.id}
                className={`relative h-1 flex-1 overflow-hidden rounded-full ${SEGMENT[state]}`}
              >
                {state === 'running' ? (
                  <span className="bar-indeterminate absolute inset-y-0 left-0 w-2/5 bg-white/50" />
                ) : null}
              </span>
            );
          })}
        </span>
      </span>
    );
  }

  return (
    <ol className="grid grid-cols-2 gap-px overflow-hidden rounded-ui border border-line bg-line sm:grid-cols-4">
      {STEPS.map((step, index) => {
        const state = stepState(deployment, step.id);
        const time = stepDuration(deployment, step.id, now);
        return (
          <li key={step.id} className="relative bg-surface px-3.5 pt-3 pb-3.5">
            <span className={`absolute inset-x-0 top-0 h-[3px] overflow-hidden ${SEGMENT[state]}`}>
              {state === 'running' ? (
                <span className="bar-indeterminate absolute inset-y-0 left-0 w-2/5 bg-white/50" />
              ) : null}
            </span>
            <span className="flex items-center gap-2">
              <span className="font-mono text-[11px] text-ink-3">
                {String(index + 1).padStart(2, '0')}
              </span>
              <span
                className={`font-medium ${state === 'waiting' || state === 'skipped' ? 'text-ink-3' : 'text-ink'}`}
              >
                {step.label}
              </span>
              {state === 'done' ? (
                <Check aria-hidden="true" className="ml-auto size-3.5 text-ok" />
              ) : null}
              {state === 'failed' ? (
                <X aria-hidden="true" className="ml-auto size-3.5 text-bad" />
              ) : null}
            </span>
            <span className="mt-1 block font-mono text-[11.5px] text-ink-3">
              {state === 'skipped' ? (
                deployment.status === 'failed' ? (
                  'Not reached'
                ) : step.id === 'build' ? (
                  'Pre-built image'
                ) : step.id === 'release' ? (
                  'No release command'
                ) : (
                  'Skipped'
                )
              ) : state === 'waiting' ? (
                'Waiting'
              ) : state === 'running' ? (
                <span className="tabular text-busy">{time ?? 'Running'}</span>
              ) : state === 'failed' ? (
                <span className="text-bad">Failed after {time}</span>
              ) : step.id === 'live' ? (
                current && deployment.status === 'live' ? (
                  'Serving traffic'
                ) : (
                  'Went live'
                )
              ) : (
                time
              )}
            </span>
          </li>
        );
      })}
    </ol>
  );
}
