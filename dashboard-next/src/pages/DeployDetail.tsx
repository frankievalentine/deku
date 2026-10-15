import { useQuery } from '@tanstack/react-query';
import { Link, useParams } from '@tanstack/react-router';
import { ArrowLeft, Undo2 } from 'lucide-react';
import { DeployProgress } from '../components/DeployProgress';
import { LogView } from '../components/LogView';
import { Button, Cli, DeployState, Section } from '../components/ui';
import { useDeployActions } from '../lib/actions';
import { absolute, deployDuration, isInProgress, shortSha, useNow } from '../lib/format';
import { q } from '../lib/queries';

const OUTCOME: Record<string, string> = {
  build: 'The build failed, so nothing was released. The previous release kept serving.',
  release:
    'The release command failed, so no new replica started. The previous release kept serving.',
  health_check:
    'A new replica did not pass its health check, so traffic never moved. The previous release kept serving.',
  live: 'Switching traffic failed. The previous release kept serving.',
};

export function DeployDetail() {
  const { app, deployId } = useParams({ from: '/apps/$app/deploys/$deployId' });
  const deployment = useQuery(q.deployment(app, deployId));
  const deployments = useQuery(q.deployments(app));
  const output = useQuery(q.deploymentLogs(app, deployId));
  const { rollback } = useDeployActions(app);
  const running = deployment.data ? isInProgress(deployment.data.status) : false;
  const now = useNow(running ? 250 : 30_000);

  if (deployment.isError) {
    return (
      <p className="text-ink-2">
        No deploy {deployId} for {app}.
      </p>
    );
  }
  if (!deployment.data) return <div className="h-40" />;

  const current = deployments.data?.find((entry) => entry.status === 'live');
  const isCurrent = current?.id === deployment.data.id;
  const d = deployment.data;

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center gap-x-5 gap-y-3">
        <Link
          to="/apps/$app/deploys"
          params={{ app }}
          className="inline-flex items-center gap-1 text-[12.5px] text-ink-3 hover:text-ink"
        >
          <ArrowLeft className="size-3.5" />
          Deploys
        </Link>
        <h2 className="font-display text-[19px]">
          Deploy <span className="font-mono">{shortSha(d.id)}</span>
        </h2>
        <DeployState status={d.status} current={isCurrent} />
        <span className="text-[12.5px] text-ink-3">
          started {absolute(d.created_at)} ·{' '}
          <span className="tabular font-mono">{deployDuration(d, now)}</span>
        </span>
        {isCurrent &&
        (deployments.data?.filter((entry) => entry.status === 'live').length ?? 0) > 1 ? (
          <Button
            className="ml-auto"
            size="sm"
            onClick={() => rollback.mutate()}
            disabled={rollback.isPending}
          >
            <Undo2 className="size-3.5" />
            Roll back this release
          </Button>
        ) : null}
      </div>

      <DeployProgress deployment={d} now={now} current={isCurrent} />

      {d.status === 'failed' && d.failed_step ? (
        <p className="rounded-ui border border-bad/35 bg-bad/[0.06] px-3.5 py-2.5 text-[13px]">
          {OUTCOME[d.failed_step]}
        </p>
      ) : null}

      <Section title="Output" meta={running ? 'streaming' : `${output.data?.length ?? 0} lines`}>
        <LogView
          lines={output.data ?? []}
          height="h-[min(560px,60vh)]"
          emptyText="Waiting for the first line."
        />
      </Section>

      <dl className="grid gap-x-8 gap-y-3 text-[13px] sm:grid-cols-3">
        <div>
          <dt className="text-[12px] text-ink-3">Image</dt>
          <dd className="mt-0.5 font-mono text-[12.5px] break-all">
            {d.image_tag ?? 'Not built yet'}
          </dd>
        </div>
        <div>
          <dt className="text-[12px] text-ink-3">Builder</dt>
          <dd className="mt-0.5">{d.builder}</dd>
        </div>
        <div>
          <dt className="text-[12px] text-ink-3">Same from the CLI</dt>
          <dd className="mt-1">
            <Cli command={`deku logs ${app} -n 200`} />
          </dd>
        </div>
      </dl>
    </div>
  );
}
