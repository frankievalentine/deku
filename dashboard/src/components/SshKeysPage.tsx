import { type SubmitEvent, useCallback, useEffect, useRef, useState } from 'react';
import { useTokenAccess } from '../hooks/useHasToken';
import { addSshKey, deleteSshKey, fetchSshKeys, type SshKey } from '../lib/api';
import ConfirmModal from './ConfirmModal';
import ConnectScreen from './ConnectScreen';
import Spinner from './Spinner';
import TableScroll from './TableScroll';

export default function SshKeysPage() {
  const tokenAccess = useTokenAccess();

  if (tokenAccess === 'unknown') {
    return null;
  }

  if (tokenAccess === 'locked') {
    return <ConnectScreen />;
  }

  return <SshKeysInner />;
}

function SshKeysInner() {
  const [keys, setKeys] = useState<SshKey[]>([]);
  const [name, setName] = useState('');
  const [publicKey, setPublicKey] = useState('');
  const [nameError, setNameError] = useState<string | null>(null);
  const [publicKeyError, setPublicKeyError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [removeTarget, setRemoveTarget] = useState<SshKey | null>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const publicKeyInput = useRef<HTMLTextAreaElement>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setLoadError(null);
      setKeys(await fetchSshKeys());
    } catch (nextError) {
      setLoadError(nextError instanceof Error ? nextError.message : 'Unable to load SSH keys.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleAdd(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextName = name.trim();
    const nextPublicKey = publicKey.trim();
    const nextNameError = nextName ? null : 'Enter a label so you can find this key later.';
    const nextPublicKeyError = nextPublicKey
      ? null
      : 'Paste the public key, starting with its type (for example ssh-ed25519).';

    setNameError(nextNameError);
    setPublicKeyError(nextPublicKeyError);
    if (nextNameError || nextPublicKeyError) {
      if (nextNameError) {
        nameInput.current?.focus();
      } else {
        publicKeyInput.current?.focus();
      }
      return;
    }

    try {
      setBusy('add');
      setActionError(null);
      await addSshKey(nextName, nextPublicKey);
      setName('');
      setPublicKey('');
      await load();
    } catch (nextError) {
      setActionError(nextError instanceof Error ? nextError.message : 'Unable to add SSH key.');
    } finally {
      setBusy(null);
    }
  }

  async function handleDelete() {
    if (!removeTarget) return;

    try {
      setBusy(removeTarget.name);
      setActionError(null);
      await deleteSshKey(removeTarget.name);
      setRemoveTarget(null);
      await load();
    } catch (nextError) {
      setActionError(nextError instanceof Error ? nextError.message : 'Unable to remove SSH key.');
      setRemoveTarget(null);
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <Spinner />
        <span>Loading SSH keys…</span>
      </div>
    );
  }

  if (loadError) {
    return (
      <div className="panel error-state">
        <p className="text-danger" role="alert">
          {loadError}
        </p>
        <button type="button" className="btn btn-secondary" onClick={() => void load()}>
          Retry loading keys
        </button>
      </div>
    );
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <h1 className="page-title">SSH keys</h1>
          <p className="page-copy">
            Register public keys that the Deku CLI uses to authenticate against this host.
          </p>
        </div>
      </section>

      <article className="panel stack-md">
        <h2 className="section-title">Add a public key</h2>
        <form onSubmit={handleAdd} className="stack-md" noValidate>
          <div className="form-group">
            <label className="form-label" htmlFor="ssh-name">
              Label
            </label>
            <input
              id="ssh-name"
              ref={nameInput}
              className="input"
              value={name}
              onChange={(event) => {
                setName(event.target.value);
                if (nameError) setNameError(null);
              }}
              placeholder="work-laptop"
              autoComplete="off"
              required
              aria-invalid={nameError ? true : undefined}
              aria-describedby={nameError ? 'ssh-name-error' : undefined}
            />
            {nameError ? (
              <p id="ssh-name-error" className="text-danger">
                {nameError}
              </p>
            ) : null}
          </div>
          <div className="form-group">
            <label className="form-label" htmlFor="ssh-key">
              Public key
            </label>
            <textarea
              id="ssh-key"
              ref={publicKeyInput}
              className="textarea"
              value={publicKey}
              onChange={(event) => {
                setPublicKey(event.target.value);
                if (publicKeyError) setPublicKeyError(null);
              }}
              placeholder="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA..."
              spellCheck={false}
              autoComplete="off"
              required
              aria-invalid={publicKeyError ? true : undefined}
              aria-describedby={publicKeyError ? 'ssh-key-error' : undefined}
            />
            {publicKeyError ? (
              <p id="ssh-key-error" className="text-danger">
                {publicKeyError}
              </p>
            ) : null}
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" type="submit" disabled={busy === 'add'}>
              {busy === 'add' ? <span className="loading-spinner" /> : null}
              <span>Add key</span>
            </button>
          </div>
        </form>
        {actionError ? (
          <p className="callout callout-danger" role="alert">
            {actionError}
          </p>
        ) : null}
      </article>

      <article className="panel stack-md">
        <h2 className="section-title">Registered keys</h2>
        {keys.length === 0 ? (
          <p className="text-muted">
            No SSH keys are registered. Add a public key above to authenticate the CLI against this
            host.
          </p>
        ) : (
          <TableScroll>
            <table className="table">
              <caption className="sr-only">SSH keys registered on this host</caption>
              <thead>
                <tr>
                  <th scope="col">Label</th>
                  <th scope="col">Fingerprint</th>
                  <th scope="col">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {keys.map((key) => (
                  <tr key={key.id}>
                    <td>{key.name}</td>
                    <td className="font-mono">{key.fingerprint ?? 'Unknown'}</td>
                    <td>
                      <button
                        type="button"
                        className="btn btn-danger btn-sm"
                        aria-label={`Remove ${key.name}`}
                        disabled={busy !== null}
                        onClick={() => setRemoveTarget(key)}
                      >
                        Remove
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableScroll>
        )}
      </article>

      <ConfirmModal
        open={removeTarget !== null}
        title={`Remove ${removeTarget?.name ?? 'key'}?`}
        description="This key stops authenticating right away. Devices still using it lose access until another key is registered."
        confirmLabel="Remove key"
        cancelLabel="Keep key"
        busy={removeTarget !== null && busy === removeTarget.name}
        onClose={() => {
          if (busy === null) setRemoveTarget(null);
        }}
        onConfirm={() => {
          void handleDelete();
        }}
      />
    </div>
  );
}
