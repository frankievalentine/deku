import { useQuery } from '@tanstack/react-query';
import { Link, Outlet, useParams, useRouterState } from '@tanstack/react-router';
import { ArrowUpRight, ChevronRight, Rocket, Undo2 } from 'lucide-react';
import { DeployProgress } from '../components/DeployProgress';
import { AppState, Button } from '../components/ui';
import { useDeployActions } from '../lib/actions';
import { isInProgress, shortSha, useNow } from '../lib/format';
import { q } from '../lib/queries';

const TABS = [
  { to: '/apps/$app', label: 'Overview', exact: true },
  { to: '/apps/$app/deploys', label: 'Deploys', exact: false },
  { to: '/apps/$app/logs', label: 'Logs', exact: false },
  { to: '/apps/$app/config', label: 'Config', exact: false },
  { to: '/apps/$app/settings', label: 'Settings', exact: false },
] as const;

export function AppLayout() {
  const { app } = useParams({ from: '/apps/$app' });
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const detail = useQuery(q.app(app));
  const deployments = useQuery(q.deployments(app));
  const { deploy, rollback } = useDeployActions(app);
  const now = useNow(1000);

  if (detail.isError) {
    return (
      <div className="mx-auto max-w-[1280px] py-16">
        <h1 className="font-display text-[22px]">No app named {app}</h1>
        <p className="mt-2 text-ink-2">
          It may have been destroyed.{' '}
          <Link to="/" className="underline">
            Back to apps
          </Link>
        </p>
      </div>
    );
  }

  const list = deployments.data ?? [];
  const active = list.find((deployment) => isInProgress(deployment.status));
  const liveCount = list.filter((deployment) => deployment.status === 'live').length;
  const domain = detail.data?.domains[0];

  return (
    <div className="mx-auto max-w-[1280px]">
      <nav
        aria-label="Breadcrumb"
        className="mb-2 flex items-center gap-1 text-[12.5px] text-ink-3"
      >
        <Link to="/" className="hover:text-ink">
          Apps
        </Link>
        <ChevronRight className="size-3" />
        <span className="font-mono text-ink-2">{app}</span>
      </nav>

      <header className="flex flex-wrap items-center gap-x-5 gap-y-3">
        <div className="min-w-0">
          <h1 className="font-display truncate text-[28px] leading-tight">{app}</h1>
          <div className="mt-1 flex flex-wrap items-center gap-x-4 gap-y-1 text-[13px] text-ink-2">
            {detail.data ? (
              <AppState
                status={detail.data.app.status}
                deploying={Boolean(active)}
                maintenance={detail.data.maintenance}
              />
            ) : null}
            {domain ? (
              <a
                href={`https://${domain.hostname}`}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 hover:text-ink"
              >
                {domain.hostname}
                <ArrowUpRight className="size-3.5" />
              </a>
            ) : null}
          </div>
        </div>

        <div className="ml-auto flex items-center gap-2">
          <Button
            onClick={() => rollback.mutate()}
            disabled={Boolean(active) || liveCount < 2 || rollback.isPending}
            title={
              liveCount < 2
                ? 'There is no earlier release to roll back to'
                : 'Redeploy the previous release'
            }
          >
            <Undo2 className="size-4" />
            Roll back
          </Button>
          <Button
            variant="primary"
            onClick={() => deploy.mutate(undefined)}
            disabled={Boolean(active) || list.length === 0 || deploy.isPending}
            title={
              list.length === 0
                ? 'Deploy an image from the overview first'
                : 'Rebuild and deploy the last source'
            }
          >
            <Rocket className="size-4" />
            Redeploy
          </Button>
        </div>
      </header>

      {active && !pathname.endsWith(`/deploys/${active.id}`) ? (
        <Link
          to="/apps/$app/deploys/$deployId"
          params={{ app, deployId: active.id }}
          className="mt-4 flex flex-wrap items-center gap-x-5 gap-y-2 rounded-ui border border-busy/40 bg-surface px-3.5 py-2.5 hover:border-busy"
        >
          <span className="text-[13px]">
            Deploy <span className="font-mono">{shortSha(active.id)}</span> in progress
          </span>
          <span className="min-w-[200px] flex-1">
            <DeployProgress deployment={active} now={now} compact />
          </span>
          <span className="text-[12.5px] text-ink-3">Watch</span>
        </Link>
      ) : null}

      <nav
        aria-label="App sections"
        className="mt-5 -mb-px flex gap-1 overflow-x-auto border-b border-line"
      >
        {TABS.map((tab) => (
          <Link
            key={tab.to}
            to={tab.to}
            params={{ app }}
            activeOptions={{ exact: tab.exact, includeSearch: false }}
            className="relative -mb-px border-b-2 border-transparent px-3 pt-1 pb-2.5 text-[13px] whitespace-nowrap text-ink-2 hover:text-ink data-[status=active]:border-accent data-[status=active]:font-medium data-[status=active]:text-ink"
          >
            {tab.label}
          </Link>
        ))}
      </nav>

      <div className="pt-6">
        <Outlet />
      </div>
    </div>
  );
}
