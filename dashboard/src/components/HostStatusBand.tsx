import { useCallback, useEffect, useMemo, useState } from 'react';
import { fetchMetricsText, parseMetrics } from '../lib/api';

export interface HostStatusInputs {
  totalApps: number;
  liveApps: number;
  readyRoutes: number;
  totalServices: number;
  alertCount: number;
}

interface HostStatusBandProps {
  inputs: HostStatusInputs;
}

interface StatusItem {
  label: string;
  value: string;
  hint?: string;
  tone?: 'neutral' | 'warning' | 'danger' | 'success';
  href?: string;
}

function metricValue(
  metrics: Map<string, Array<{ value: number; labels: Record<string, string> }>>,
  name: string
): number | null {
  const samples = metrics.get(name);
  if (!samples || samples.length === 0) return null;
  return samples[0].value;
}

/**
 * The single summary band at the top of the overview. It merges the counts that
 * come from the host state with the ones the daemon exposes as metrics, so the
 * page does not present the same numbers twice.
 */
export default function HostStatusBand({ inputs }: HostStatusBandProps) {
  const [metrics, setMetrics] = useState<Map<
    string,
    Array<{ value: number; labels: Record<string, string> }>
  > | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setError(null);
      setMetrics(parseMetrics(await fetchMetricsText()));
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to load metrics.');
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const items = useMemo<StatusItem[]>(() => {
    const value = (name: string) => (metrics ? metricValue(metrics, name) : null);

    const containers = value('deku_containers_running');
    const liveDeployments = value('deku_deployments_live');
    const backups = value('deku_service_backups_total');
    const overdue = value('deku_backup_schedules_overdue');
    const tlsApps = value('deku_apps_tls_enabled');

    const list: StatusItem[] = [
      {
        label: 'Apps',
        value: String(inputs.totalApps),
        hint: `${inputs.liveApps} live`,
      },
      {
        label: 'Routes ready',
        value: `${inputs.readyRoutes}/${inputs.totalApps}`,
      },
      {
        label: 'Services',
        value: String(inputs.totalServices),
      },
    ];

    if (containers !== null) {
      list.push({
        label: 'Containers',
        value: String(containers),
        hint: liveDeployments !== null ? `${liveDeployments} live deployments` : undefined,
      });
    }

    if (tlsApps !== null) {
      list.push({ label: 'Apps with TLS', value: String(tlsApps) });
    }

    if (backups !== null) {
      list.push({
        label: 'Backups',
        value: String(backups),
        hint: overdue ? `${overdue} overdue` : undefined,
        tone: overdue ? 'warning' : 'neutral',
        href: overdue ? '/storage' : undefined,
      });
    }

    list.push({
      label: 'Active alerts',
      value: String(inputs.alertCount),
      tone: inputs.alertCount > 0 ? 'warning' : 'success',
    });

    return list;
  }, [inputs, metrics]);

  return (
    <section className="hero-panel">
      <div className="panel-heading panel-heading-top">
        <div className="stack-md">
          <h1 className="page-title">Server overview</h1>
          <p className="page-copy">What is running, what needs attention, and recent activity.</p>
        </div>
        <button
          className="btn btn-secondary btn-sm"
          type="button"
          onClick={() => {
            void load();
          }}
        >
          Refresh
        </button>
      </div>

      {error ? <p className="callout callout-danger">{error}</p> : null}

      <ul className="status-band">
        {items.map((item) => {
          const content = (
            <>
              <strong>{item.value}</strong>
              <span>{item.label}</span>
              {item.hint ? <small>{item.hint}</small> : null}
            </>
          );
          return (
            <li
              key={item.label}
              className={`status-band-item${item.tone ? ` status-band-item-${item.tone}` : ''}`}
            >
              {item.href ? <a href={item.href}>{content}</a> : content}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
