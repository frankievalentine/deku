import { type SubmitEvent, useState } from 'react';
import { setToken, verifyToken } from '../lib/api';
import { showToast } from '../lib/shell';

interface ConnectScreenProps {
  onConnected?: () => void;
}

export default function ConnectScreen({ onConnected }: ConnectScreenProps) {
  const [token, setTokenValue] = useState('');
  const [loading, setLoading] = useState(false);

  async function handleSubmit(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmed = token.trim();
    if (!trimmed) {
      showToast({
        title: 'Dashboard token required',
        description: 'Paste the one-time dashboard token before connecting.',
        variant: 'warning',
      });
      return;
    }

    setLoading(true);

    try {
      const valid = await verifyToken(trimmed);
      if (!valid) {
        showToast({
          title: 'Unable to connect',
          description:
            'Invalid token. Use the one-time token from setup, or run `deku dashboard reset-token` on the host.',
          variant: 'error',
        });
        return;
      }

      setToken(trimmed);
      showToast({
        title: 'Logged in successfully',
        description: 'The token is stored locally in this browser.',
        variant: 'success',
      });
      onConnected?.();
    } catch {
      showToast({
        title: 'Unable to connect',
        description: 'Unable to reach the daemon. Confirm the API is reachable from this browser.',
        variant: 'error',
      });
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="connect-shell">
      <div className="connect-card card">
        <div className="stack-md">
          <div className="connect-badge">Control handoff</div>
          <div className="stack-sm">
            <h1 className="connect-title">Bind this dashboard to your daemon</h1>
            <p className="connect-copy">
              Paste the one-time dashboard token shown during <code>deku setup</code> or a later{' '}
              <code>deku dashboard reset-token</code>. It is stored only in this browser after you
              connect.
            </p>
          </div>

          <form onSubmit={handleSubmit} className="stack-md" noValidate>
            <div className="form-group">
              <label htmlFor="token-input" className="form-label">
                Dashboard Token
              </label>
              <input
                id="token-input"
                type="password"
                className="input"
                placeholder="dku_..."
                value={token}
                onChange={(event) => setTokenValue(event.target.value)}
                autoComplete="current-password"
                spellCheck={false}
              />
            </div>

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
    </div>
  );
}
