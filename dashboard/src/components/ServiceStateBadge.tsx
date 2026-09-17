interface ServiceStateBadgeProps {
  status: string;
  label?: string;
}

export default function ServiceStateBadge({ status, label }: ServiceStateBadgeProps) {
  return (
    <span className={`service-state service-state-${serviceTone(status)}`}>
      {label ? `${label}: ${status}` : status}
    </span>
  );
}

function serviceTone(status: string): 'success' | 'warning' | 'danger' {
  const normalized = status.toLowerCase();

  if (
    normalized.includes('run') ||
    normalized.includes('ready') ||
    normalized.includes('valid') ||
    normalized.includes('up')
  ) {
    return 'success';
  }

  if (
    normalized.includes('fail') ||
    normalized.includes('error') ||
    normalized.includes('invalid') ||
    normalized.includes('degraded')
  ) {
    return 'danger';
  }

  return 'warning';
}
