import { type SubmitEvent, useCallback, useEffect, useState } from 'react';
import {
  createDeployToken,
  type DeployToken,
  fetchDeployTokens,
  type NewDeployToken,
  revokeDeployToken,
} from '../lib/api';
import { copyText } from '../lib/shell';
import ConfirmModal from './ConfirmModal';
import Spinner from './Spinner';
import TableScroll from './TableScroll';

interface AppDeployTokensPanelProps {
  appName: string;
  locked: boolean;
}

function formatDate(value: string | null): string {
  if (!value) return 'Never';
  return new Date(value).toLocaleString();
}

export default function AppDeployTokensPanel({ appName, locked }: AppDeployTokensPanelProps) {
  const [tokens, setTokens] = useState<DeployToken[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [name, setName] = useState('');
  const [newToken, setNewToken] = useState<NewDeployToken | null>(null);
  const [copied, setCopied] = useState(false);
  const [tokenToRevoke, setTokenToRevoke] = useState<DeployToken | null>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      setTokens(await fetchDeployTokens(appName));
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to load deploy tokens.');
    } finally {
      setLoading(false);
    }
  }, [appName]);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleCreate(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (locked) return;

    const label = name.trim();
    if (!label) {
      setError('Give the token a name so you can tell it apart later.');
      return;
    }

    try {
      setBusy('create');
      setError(null);
      setNotice(null);
      setCopied(false);
      const created = await createDeployToken(appName, label);
      setNewToken(created);
      setName('');
      await load();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to create the token.');
    } finally {
      setBusy(null);
    }
  }

  async function handleRevoke() {
    if (!tokenToRevoke) return;
    const token = tokenToRevoke;
    try {
      setBusy(`revoke-${token.id}`);
      setError(null);
      setNotice(null);
      await revokeDeployToken(appName, token.id);
      setTokenToRevoke(null);
      await load();
      setNotice(`Revoked ${token.name}. It can no longer deploy this app.`);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to revoke the token.');
    } finally {
      setBusy(null);
    }
  }

  return (
    <>
      <article className="panel stack-md">
        <div className="panel-heading">
          <div className="stack-sm panel-heading-copy">
            <p className="eyebrow">Deploy access</p>
            <h2 className="section-title">Deploy tokens</h2>
            <p className="page-copy">
              Tokens let a CI job or teammate deploy this app without the dashboard token. The
              secret is shown once, when you create it.
            </p>
          </div>
          <span className="inventory-summary">
            {tokens.length} token{tokens.length === 1 ? '' : 's'}
          </span>
        </div>

        {notice ? <p className="callout callout-success">{notice}</p> : null}
        {error ? <p className="callout callout-danger">{error}</p> : null}

        {newToken ? (
          <div className="callout callout-warning stack-sm">
            <p>
              Copy this token now. You will not be able to see it again after you leave this page.
            </p>
            <div className="cluster">
              <code className="font-mono">{newToken.token}</code>
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                onClick={() => {
                  void copyText(newToken.token).then((ok) => {
                    setCopied(ok);
                    if (!ok) setError('Copying failed. Select the token and copy it manually.');
                  });
                }}
              >
                {copied ? 'Copied' : 'Copy token'}
              </button>
              <button
                className="btn btn-ghost btn-sm"
                type="button"
                onClick={() => {
                  setNewToken(null);
                  setCopied(false);
                }}
              >
                Done
              </button>
            </div>
          </div>
        ) : null}

        {loading ? (
          <div className="loading-state">
            <Spinner />
            <span>Loading deploy tokens…</span>
          </div>
        ) : tokens.length === 0 ? (
          <p className="text-muted">No deploy tokens yet.</p>
        ) : (
          <TableScroll>
            <table className="table">
              <caption className="sr-only">Deploy tokens for this app</caption>
              <thead>
                <tr>
                  <th scope="col">Name</th>
                  <th scope="col">Prefix</th>
                  <th scope="col">Created</th>
                  <th scope="col">Last used</th>
                  <th scope="col">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {tokens.map((token) => (
                  <tr key={token.id}>
                    <td>{token.name}</td>
                    <td className="font-mono">{token.prefix}…</td>
                    <td>{formatDate(token.created_at)}</td>
                    <td>{formatDate(token.last_used_at)}</td>
                    <td>
                      <button
                        className="btn btn-outline btn-danger-outline btn-sm"
                        type="button"
                        aria-label={`Revoke token ${token.name}`}
                        onClick={() => setTokenToRevoke(token)}
                        disabled={locked || busy !== null}
                      >
                        Revoke
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableScroll>
        )}

        <form onSubmit={handleCreate} className="stack-md" noValidate>
          <div className="form-group">
            <label className="form-label" htmlFor="deploy-token-name">
              New token name
            </label>
            <input
              id="deploy-token-name"
              className="input"
              placeholder="github-actions"
              value={name}
              onChange={(event) => setName(event.target.value)}
              disabled={locked || busy !== null}
            />
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" type="submit" disabled={locked || busy !== null}>
              {busy === 'create' ? <span className="loading-spinner" /> : null}
              <span>Create token</span>
            </button>
          </div>
        </form>
      </article>

      <ConfirmModal
        open={tokenToRevoke !== null}
        title="Revoke this token?"
        description={
          tokenToRevoke
            ? `Anything using ${tokenToRevoke.name} will stop being able to deploy ${appName}.`
            : ''
        }
        confirmLabel="Revoke token"
        busy={tokenToRevoke ? busy === `revoke-${tokenToRevoke.id}` : false}
        onClose={() => {
          if (!busy?.startsWith('revoke-')) setTokenToRevoke(null);
        }}
        onConfirm={() => {
          void handleRevoke();
        }}
      />
    </>
  );
}
