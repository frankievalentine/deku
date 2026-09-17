import { type SubmitEvent, useRef, useState } from 'react';
import { setToken, verifyToken } from '../lib/api';
import { showToast } from '../lib/shell';

interface ConnectScreenProps {
  onConnected?: () => void;
}

export default function ConnectScreen({ onConnected }: ConnectScreenProps) {
  const [token, setTokenValue] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  async function handleSubmit(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmed = token.trim();

    if (!trimmed) {
      setError('Paste the dashboard token to connect.');
      inputRef.current?.focus();
      return;
    }

    setError(null);
    setLoading(true);

    try {
      const valid = await verifyToken(trimmed);
      if (!valid) {
        setError(
          'That token was rejected. Run `deku dashboard reset-token` and paste the new one.'
        );
        inputRef.current?.focus();
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
      setError('Unable to reach the daemon. Confirm the API is reachable from this browser.');
      inputRef.current?.focus();
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
                ref={inputRef}
                type="password"
                className="input"
                placeholder="dku_..."
                value={token}
                onChange={(event) => setTokenValue(event.target.value)}
                autoComplete="current-password"
                spellCheck={false}
                aria-invalid={error ? true : undefined}
                aria-describedby={error ? 'token-error' : undefined}
              />
              {error ? (
                <p id="token-error" className="field-error" role="alert">
                  {error}
                </p>
              ) : null}
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
