import type { App, Deployment } from '../lib/api';

type Status = App['status'] | Deployment['status'];

interface StatusBadgeProps {
  status: Status;
  size?: 'sm' | 'md';
}

const STATUS_META: Record<Status, { label: string; color: string }> = {
  created: { label: 'Created', color: 'var(--color-status-pending)' },
  deployed: { label: 'Deployed', color: 'var(--color-status-running)' },
  stopped: { label: 'Stopped', color: 'var(--color-status-stopped)' },
  error: { label: 'Error', color: 'var(--color-status-error)' },
  pending: { label: 'Pending', color: 'var(--color-status-pending)' },
  building: { label: 'Building', color: 'var(--color-status-building)' },
  built: { label: 'Built', color: 'var(--color-status-building)' },
  deploying: { label: 'Deploying', color: 'var(--color-status-building)' },
  health_checking: { label: 'Health check', color: 'var(--color-status-building)' },
  live: { label: 'Live', color: 'var(--color-status-running)' },
  failed: { label: 'Failed', color: 'var(--color-status-error)' },
  rolled_back: { label: 'Rolled back', color: 'var(--color-status-stopped)' },
};

export default function StatusBadge({ status, size = 'md' }: StatusBadgeProps) {
  const meta = STATUS_META[status];
  const color = meta?.color ?? 'var(--color-text-muted)';
  const label = meta?.label ?? status;
  const animated = ['building', 'deploying', 'health_checking'].includes(status);

  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: '5px',
        padding: size === 'sm' ? '2px 7px' : '3px 10px',
        borderRadius: 'var(--radius-pill)',
        fontSize: size === 'sm' ? '0.72rem' : '0.78rem',
        fontWeight: 500,
        color,
        background: `color-mix(in srgb, ${color} 14%, transparent)`,
        border: `1px solid color-mix(in srgb, ${color} 30%, transparent)`,
        whiteSpace: 'nowrap',
        lineHeight: 1.4,
        fontFamily: 'var(--font-mono)',
        textTransform: 'uppercase',
        letterSpacing: '0.08em',
      }}
      aria-label={`Status: ${label}`}
    >
      <span
        style={{
          width: '6px',
          height: '6px',
          borderRadius: '50%',
          background: color,
          flexShrink: 0,
          ...(animated ? { animation: 'pulse 1.2s ease-in-out infinite' } : {}),
        }}
      />
      {label}
      <style>{`
        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.3; }
        }
      `}</style>
    </span>
  );
}
