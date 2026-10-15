import { useEffect, useState } from 'react';
import type { Deployment, DeployStatus } from './types';

const UNITS: Array<[Intl.RelativeTimeFormatUnit, number]> = [
  ['day', 86_400_000],
  ['hour', 3_600_000],
  ['minute', 60_000],
];

/** "3h ago", "just now": compact enough for a dense row. */
export function relative(iso: string, now = Date.now()): string {
  const delta = now - new Date(iso).getTime();
  if (delta < 45_000) return 'just now';
  for (const [unit, size] of UNITS) {
    if (delta >= size) {
      const value = Math.floor(delta / size);
      const suffix = unit === 'day' ? 'd' : unit === 'hour' ? 'h' : 'm';
      return `${value}${suffix} ago`;
    }
  }
  return 'just now';
}

export function absolute(iso: string): string {
  return new Date(iso).toLocaleString('en-US', {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

export function clock(iso: string): string {
  return new Date(iso).toLocaleTimeString('en-US', { hour12: false });
}

export function duration(ms: number): string {
  if (ms < 1000) return `${Math.max(0, Math.round(ms))}ms`;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ${String(seconds % 60).padStart(2, '0')}s`;
}

export function deployDuration(deployment: Deployment, now = Date.now()): string {
  const end = deployment.finished_at ? new Date(deployment.finished_at).getTime() : now;
  return duration(end - new Date(deployment.created_at).getTime());
}

export function uptime(seconds: number): string {
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3600);
  return days > 0 ? `${days}d ${hours}h` : `${hours}h`;
}

/** Re-renders the caller on an interval so relative times and running timers stay current. */
export function useNow(intervalMs = 30_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);
  return now;
}

const IN_PROGRESS: DeployStatus[] = [
  'pending',
  'building',
  'built',
  'deploying',
  'health_checking',
];

export function isInProgress(status: DeployStatus): boolean {
  return IN_PROGRESS.includes(status);
}

export const STATUS_LABEL: Record<DeployStatus, string> = {
  pending: 'Queued',
  building: 'Building',
  built: 'Built',
  deploying: 'Releasing',
  health_checking: 'Health checking',
  live: 'Live',
  failed: 'Failed',
  rolled_back: 'Rolled back',
};

export function shortSha(id: string): string {
  return id.slice(0, 7);
}
