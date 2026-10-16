import { useQuery } from '@tanstack/react-query';
import { Link, useNavigate, useParams } from '@tanstack/react-router';
import { useState } from 'react';
import { client } from '../api/client';
import { DeployProgress, STEPS, stepState } from '../components/DeployProgress';
import { Button, Cli, DeployState, Empty, Field, Input } from '../components/ui';
import { useAction, useDeployActions } from '../lib/actions';
import { absolute, deployDuration, isInProgress, relative, shortSha, useNow } from '../lib/format';
import { q } from '../lib/queries';
import type { Deployment } from '../lib/types';

const TRIGGER: Record<NonNullable<Deployment['trigger']>, string> = {
  cli: 'CLI',
  git: 'git push',
  token: 'Deploy token',
  dashboard: 'Dashboard',
};

const FAILED_AT: Record<string, string> = {
  build: 'at build',
  release: 'at release',
  health_check: 'at health check',
  live: 'switching traffic',
};

function dayLabel(iso: string, now: number): string {
  const date = new Date(iso);
  const today = new Date(now);
  const yesterday = new Date(now - 86_400_000);
  if (date.toDateString() === today.toDateString()) return 'Today';
  if (date.toDateString() === yesterday.toDateString()) return 'Yesterday';
  return date.toLocaleDateString('en-US', { weekday: 'long', month: 'short', day: 'numeric' });
}

export function AppDeploys() {
  const { app } = useParams({ from: '/apps/$app' });
  const deployments = useQuery(q.deployments(app));
  const now = useNow(1000);
  const list = deployments.data ?? [];
  const currentId = list.find((deployment) => deployment.status === 'live')?.id;

  if (deployments.isSuccess && list.length === 0) {
    return (
      <div className="flex flex-col gap-6">
        <NewDeploy app={app} />
        <Empty title="No deploys yet" command={`deku deploy run ${app} --path .`}>
          Every deploy is kept here with its build output, so you can see what changed and roll
          back.
        </Empty>
      </div>
    );
  }

  const groups: Array<[string, Deployment[]]> = [];
  for (const deployment of list) {
    const label = dayLabel(deployment.created_at, now);
    const last = groups.at(-1);
    if (last && last[0] === label) last[1].push(deployment);
    else groups.push([label, [deployment]]);
  }

  return (
    <div className="flex flex-col gap-6">
      <NewDeploy app={app} />
      <div className="flex flex-wrap items-center gap-3">
        <p className="text-ink-2">
          {list.length} deploys. A failed deploy never replaces the release that is serving.
        </p>
        <Cli className="ml-auto" command={`deku deploy list ${app}`} />
      </div>
      {groups.map(([label, entries]) => (
        <section key={label}>
          <h2 className="mb-1 font-mono text-[10.5px] tracking-[0.08em] text-ink-3 uppercase">
            {label}
          </h2>
          <ul className="border-t border-line">
            {entries.map((deployment) => (
              <li key={deployment.id} className="border-b border-line">
                <Link
                  to="/apps/$app/deploys/$deployId"
                  params={{ app, deployId: deployment.id }}
                  className="grid grid-cols-[1fr_auto] items-center gap-x-5 gap-y-1.5 px-2 py-3 hover:bg-sunk/60 md:grid-cols-[150px_80px_minmax(0,1fr)_170px_70px]"
                >
                  <span className="flex flex-col gap-0.5">
                    <DeployState status={deployment.status} current={deployment.id === currentId} />
                    {deployment.status === 'failed' && deployment.failed_step ? (
                      <span className="pl-[13px] text-[12px] text-ink-3">
                        {FAILED_AT[deployment.failed_step]}
                      </span>
                    ) : null}
                  </span>
                  <span className="text-right font-mono text-[12.5px] md:text-left">
                    {shortSha(deployment.id)}
                  </span>
                  <span className="col-span-2 min-w-0 text-[12.5px] text-ink-2 md:col-span-1">
                    <span className="block truncate">
                      {deployment.builder}
                      {deployment.trigger ? ` · ${TRIGGER[deployment.trigger]}` : ''}
                    </span>
                    <span className="block truncate font-mono text-[11.5px] text-ink-3">
                      {deployment.image_tag}
                    </span>
                  </span>
                  <span className="hidden md:block">
                    {isInProgress(deployment.status) ? (
                      <DeployProgress deployment={deployment} now={now} compact />
                    ) : (
                      <StepTicks deployment={deployment} />
                    )}
                  </span>
                  <span
                    className="hidden text-right text-[12.5px] text-ink-3 md:block"
                    title={absolute(deployment.created_at)}
                  >
                    <span className="block">{relative(deployment.created_at, now)}</span>
                    <span className="tabular block font-mono text-[11.5px]">
                      {deployDuration(deployment, now)}
                    </span>
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

function StepTicks({ deployment }: { deployment: Deployment }) {
  return (
    <span className="flex max-w-[180px] gap-[3px]" role="img" aria-label="Deploy steps">
      {STEPS.map((step) => {
        const state = stepState(deployment, step.id);
        const color = state === 'failed' ? 'bg-bad' : state === 'done' ? 'bg-ink-3/50' : 'bg-line';
        return (
          <span key={step.id} title={step.label} className={`h-1 flex-1 rounded-full ${color}`} />
        );
      })}
    </span>
  );
}

/** Deploy a pre-built image, or upload a source archive for Deku to build. */
function NewDeploy({ app }: { app: string }) {
  const [image, setImage] = useState('');
  const [archive, setArchive] = useState<File | null>(null);
  const navigate = useNavigate();
  const { deploy } = useDeployActions(app);
  const upload = useAction(
    (chosen: File) => client.uploadArchive(app, chosen),
    `Uploaded. Building ${app}.`
  );

  return (
    <section
      aria-label="New deploy"
      className="rounded-ui grid gap-5 border border-line bg-surface p-4 lg:grid-cols-2"
    >
      <form
        className="flex flex-col gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (image.trim()) deploy.mutate(image.trim());
        }}
      >
        <Field label="Deploy an image" hint="Any registry the server can pull from.">
          <Input
            mono
            value={image}
            onChange={(e) => setImage(e.target.value)}
            placeholder="ghcr.io/acme/web:1.4.2"
          />
        </Field>
        <div className="flex flex-wrap items-center gap-2">
          <Button type="submit" disabled={!image.trim() || deploy.isPending}>
            Deploy image
          </Button>
          <Cli command={`deku deploy run ${app} --image ${image.trim() || '<ref>'}`} />
        </div>
      </form>
      <form
        className="flex flex-col gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (!archive) return;
          upload.mutate(archive, {
            onSuccess: (deployment) =>
              navigate({
                to: '/apps/$app/deploys/$deployId',
                params: { app, deployId: deployment.id },
              }),
          });
        }}
      >
        <Field
          label="Upload source"
          hint="A .tar.gz or .zip of the project. Deku builds it with its Dockerfile, or works out how."
        >
          <input
            type="file"
            accept=".tar.gz,.tgz,.zip"
            onChange={(e) => setArchive(e.target.files?.[0] ?? null)}
            className="rounded-ui h-8 border border-line-2 bg-bg px-1.5 text-[12.5px] file:mr-2 file:rounded-[calc(var(--radius)-2px)] file:border-0 file:bg-sunk file:px-2 file:py-0.5 file:text-[12.5px] file:text-ink"
          />
        </Field>
        <div className="flex flex-wrap items-center gap-2">
          <Button type="submit" disabled={!archive || upload.isPending}>
            Upload and deploy
          </Button>
          <Cli command={`deku deploy run ${app} --path .`} />
        </div>
      </form>
    </section>
  );
}
