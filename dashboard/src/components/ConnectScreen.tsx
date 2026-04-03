import { useState, type FormEvent } from 'react';
import { setToken, verifyToken } from '../lib/api';

interface ConnectScreenProps {
  onConnected: () => void;
}

export default function ConnectScreen({ onConnected }: ConnectScreenProps) {
  const [token, setTokenValue] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmed = token.trim();
    if (!trimmed) {
      setError('Token cannot be empty.');
      return;
    }

    setLoading(true);
    setError(null);

    try {
      const valid = await verifyToken(trimmed);
      if (!valid) {
        setError('Invalid token. Use the one-time token from setup, or run `deku dashboard reset-token` on the host.');
        return;
      }

      setToken(trimmed);
      onConnected();
    } catch {
      setError('Unable to reach the daemon. Confirm the API is reachable from this browser.');
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="connect-shell">
      <div className="connect-card">
        <div className="connect-badge">Control handoff</div>
        <div className="connect-logo">
          <span className="connect-wordmark">DEKU</span>
        </div>
        <h1 className="connect-title">Bind this dashboard to your daemon</h1>
        <p className="connect-copy">
          Paste the one-time dashboard token shown during <code>deku setup</code> or a later{' '}
          <code>deku dashboard reset-token</code>. It is stored only in this browser after you connect.
        </p>

        <form onSubmit={handleSubmit} className="stack-md" noValidate>
          <div className="form-group">
            <label htmlFor="token-input" className="form-label">
              Dashboard Token
            </label>
            <input
              id="token-input"
              type="password"
              className="input"
              placeholder="eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
              value={token}
              onChange={(event) => setTokenValue(event.target.value)}
              autoComplete="current-password"
              autoFocus
              spellCheck={false}
            />
          </div>

          {error && (
            <p className="text-danger" role="alert">
              {error}
            </p>
          )}

          <button type="submit" className="btn btn-primary btn-block" disabled={loading}>
            {loading ? (
              <>
                <span className="loading-spinner" />
                Connecting…
              </>
            ) : (
              'Connect'
            )}
          </button>
        </form>
      </div>
    </div>
  );
}
