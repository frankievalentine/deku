import { type FormEvent, useEffect, useState } from 'react';
import { type SshKey, addSshKey, deleteSshKey, fetchSshKeys, getToken } from '../lib/api';
import ConnectScreen from './ConnectScreen';

export default function SshKeysPage() {
  const [hasToken, setHasToken] = useState(() => Boolean(getToken()));

  if (!hasToken) {
    return <ConnectScreen onConnected={() => setHasToken(true)} />;
  }

  return <SshKeysInner />;
}

function SshKeysInner() {
  const [keys, setKeys] = useState<SshKey[]>([]);
  const [name, setName] = useState('');
  const [publicKey, setPublicKey] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void load();
  }, []);

  async function load() {
    try {
      setLoading(true);
      setError(null);
      setKeys(await fetchSshKeys());
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to load SSH keys.');
    } finally {
      setLoading(false);
    }
  }

  async function handleAdd(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!name.trim() || !publicKey.trim()) return;
    try {
      setBusy('add');
      await addSshKey(name.trim(), publicKey.trim());
      setName('');
      setPublicKey('');
      await load();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to add SSH key.');
    } finally {
      setBusy(null);
    }
  }

  async function handleDelete(keyName: string) {
    try {
      setBusy(keyName);
      await deleteSshKey(keyName);
      await load();
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : 'Unable to remove SSH key.');
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return (
      <div className="panel loading-state">
        <span className="loading-spinner" />
        <span>Loading SSH key inventory…</span>
      </div>
    );
  }

  return (
    <div className="stack-lg">
      <section className="hero-panel">
        <div className="stack-md">
          <p className="eyebrow">Access control</p>
          <h1 className="page-title">SSH keys</h1>
          <p className="page-copy">Register public keys for CLI auth and server operations.</p>
        </div>
      </section>

      <article className="panel stack-md">
        <div className="stack-sm">
          <p className="eyebrow">Add key</p>
          <h2 className="section-title">Trusted public key</h2>
        </div>
        <form onSubmit={handleAdd} className="stack-md">
          <div className="form-group">
            <label className="form-label" htmlFor="ssh-name">
              Label
            </label>
            <input
              id="ssh-name"
              className="input"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="work-laptop"
            />
          </div>
          <div className="form-group">
            <label className="form-label" htmlFor="ssh-key">
              Public key
            </label>
            <textarea
              id="ssh-key"
              className="textarea"
              value={publicKey}
              onChange={(event) => setPublicKey(event.target.value)}
              placeholder="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA..."
            />
          </div>
          <div className="form-actions">
            <button className="btn btn-primary" type="submit" disabled={busy === 'add'}>
              {busy === 'add' ? 'Adding…' : 'Add key'}
            </button>
          </div>
        </form>
        {error && <p className="text-danger">{error}</p>}
      </article>

      <article className="panel stack-md">
        <div className="stack-sm">
          <p className="eyebrow">Inventory</p>
          <h2 className="section-title">Registered keys</h2>
        </div>
        {keys.length === 0 ? (
          <p className="text-muted">No SSH keys have been added yet.</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Fingerprint</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {keys.map((key) => (
                <tr key={key.id}>
                  <td>{key.name}</td>
                  <td className="font-mono">{key.fingerprint ?? 'unavailable'}</td>
                  <td>
                    <button
                      type="button"
                      className="btn btn-danger btn-sm"
                      disabled={busy === key.name}
                      onClick={() => handleDelete(key.name)}
                    >
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </article>
    </div>
  );
}
