import { type SubmitEvent, useCallback, useEffect, useRef, useState } from 'react';
import type { NetworkRecord } from '../lib/api';
import { createNetwork, deleteNetwork, fetchNetworks } from '../lib/api';
import ConfirmModal from './ConfirmModal';
import Spinner from './Spinner';
import TableScroll from './TableScroll';

export default function HostNetworksPanel() {
  const [networks, setNetworks] = useState<NetworkRecord[]>([]);
  const [name, setName] = useState('');
  const [nameError, setNameError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [removeTarget, setRemoveTarget] = useState<NetworkRecord | null>(null);
  const nameInput = useRef<HTMLInputElement>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setLoadError(null);
      setNetworks(await fetchNetworks());
    } catch (nextError) {
      setLoadError(nextError instanceof Error ? nextError.message : 'Unable to load networks.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleCreate(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextName = name.trim();
    if (!nextName) {
      setNameError('Enter a network name.');
      nameInput.current?.focus();
      return;
    }

    try {
      setBusy('create');
      setActionError(null);
      setNotice(null);
      await createNetwork(nextName);
      setName('');
      await load();
      setNotice(`Created network ${nextName}.`);
    } catch (nextError) {
      setActionError(nextError instanceof Error ? nextError.message : 'Unable to create network.');
    } finally {
      setBusy(null);
    }
  }

  async function handleDelete() {
    if (!removeTarget) return;

    try {
      setBusy(`delete-${removeTarget.name}`);
      setActionError(null);
      setNotice(null);
      await deleteNetwork(removeTarget.name);
      setRemoveTarget(null);
      await load();
      setNotice(`Deleted network ${removeTarget.name}.`);
    } catch (nextError) {
      setActionError(nextError instanceof Error ? nextError.message : 'Unable to delete network.');
      setRemoveTarget(null);
    } finally {
      setBusy(null);
    }
  }

  return (
    <>
      <article className="panel stack-md">
        <div className="cluster justify-between align-center">
          <div className="stack-sm">
            <p className="eyebrow">Networks</p>
            <h2 className="section-title">Docker networks</h2>
            <p className="page-copy">
              Host-wide Docker networks that apps can attach to. Deleting one detaches every app
              using it.
            </p>
          </div>
          <span className="inventory-summary">{networks.length} networks</span>
        </div>

        {notice ? <p className="callout callout-success">{notice}</p> : null}
        {actionError ? <p className="callout callout-danger">{actionError}</p> : null}

        {loadError ? (
          <div className="error-state">
            <p className="text-danger" role="alert">
              {loadError}
            </p>
            <button type="button" className="btn btn-secondary" onClick={() => void load()}>
              Retry loading networks
            </button>
          </div>
        ) : (
          <>
            <form onSubmit={handleCreate} className="stack-md" noValidate>
              <div className="form-group">
                <label className="form-label" htmlFor="host-network-name">
                  New network
                </label>
                <input
                  id="host-network-name"
                  ref={nameInput}
                  className="input"
                  value={name}
                  onChange={(event) => {
                    setName(event.target.value);
                    if (nameError) setNameError(null);
                  }}
                  placeholder="private-backplane"
                  autoComplete="off"
                  required
                  aria-invalid={nameError ? true : undefined}
                  aria-describedby={nameError ? 'host-network-name-error' : undefined}
                  disabled={busy !== null}
                />
                {nameError ? (
                  <p id="host-network-name-error" className="text-danger">
                    {nameError}
                  </p>
                ) : null}
              </div>
              <div className="form-actions">
                <button className="btn btn-primary" type="submit" disabled={busy !== null}>
                  {busy === 'create' ? <span className="loading-spinner" /> : null}
                  <span>Create network</span>
                </button>
              </div>
            </form>

            {loading ? (
              <div className="loading-state">
                <Spinner />
                <span>Loading networks…</span>
              </div>
            ) : networks.length === 0 ? (
              <p className="text-muted">
                No networks exist yet. Create one above to attach apps to it.
              </p>
            ) : (
              <TableScroll>
                <table className="table">
                  <caption className="sr-only">Docker networks on this host</caption>
                  <thead>
                    <tr>
                      <th scope="col">Name</th>
                      <th scope="col">
                        <span className="sr-only">Actions</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {networks.map((network) => (
                      <tr key={network.id}>
                        <td>{network.name}</td>
                        <td>
                          <button
                            type="button"
                            className="btn btn-outline btn-danger-outline btn-sm"
                            aria-label={`Delete network ${network.name}`}
                            disabled={busy === `delete-${network.name}`}
                            onClick={() => setRemoveTarget(network)}
                          >
                            Delete
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </TableScroll>
            )}
          </>
        )}
      </article>

      <ConfirmModal
        open={removeTarget !== null}
        title={`Delete ${removeTarget?.name ?? 'network'}?`}
        description={`This removes the network ${removeTarget?.name ?? ''} and detaches every app currently using it.`}
        confirmLabel="Delete network"
        cancelLabel="Keep network"
        busy={removeTarget !== null && busy === `delete-${removeTarget.name}`}
        onClose={() => {
          if (busy === null) setRemoveTarget(null);
        }}
        onConfirm={() => {
          void handleDelete();
        }}
      />
    </>
  );
}
