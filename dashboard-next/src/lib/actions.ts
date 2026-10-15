import { useMutation } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { toast } from '../components/ui';
import { client } from '../mock/api';

/** Deploy and roll back for one app. Both land on the new deploy so it can be watched. */
export function useDeployActions(app: string) {
  const navigate = useNavigate();
  const watch = (deployId: string) =>
    navigate({ to: '/apps/$app/deploys/$deployId', params: { app, deployId } });

  const deploy = useMutation({
    mutationFn: (image?: string) => client.startDeploy(app, image),
    onSuccess: (deployment) => {
      toast(`Deploying ${app}`, 'busy');
      void watch(deployment.id);
    },
    onError: (error) => toast(error.message, 'bad'),
  });

  const rollback = useMutation({
    mutationFn: () => client.rollback(app),
    onSuccess: (deployment) => {
      toast(`Rolling back ${app} to ${deployment.image_tag}`, 'busy');
      void watch(deployment.id);
    },
    onError: (error) => toast(error.message, 'bad'),
  });

  return { deploy, rollback };
}

/**
 * A mutation that reports its outcome as a toast. `success` is the confirmation text; a failure
 * shows the error, which the client words as an instruction.
 */
export function useAction<TArgs, TResult>(
  run: (args: TArgs) => Promise<TResult>,
  success?: string | ((result: TResult, args: TArgs) => string)
) {
  return useMutation({
    mutationFn: run,
    onSuccess: (result, args) => {
      if (success) toast(typeof success === 'string' ? success : success(result, args));
    },
    onError: (error) => toast(error.message, 'bad'),
  });
}
