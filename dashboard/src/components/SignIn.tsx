import { useState } from 'react';
import { client } from '../api/client';
import { Button, Cli, Field, Input } from './ui';

/**
 * Exchange the dashboard token for a session. The token is sent once and kept
 * nowhere: afterwards the browser holds only an HttpOnly cookie.
 */
export function SignIn({ onSignedIn }: { onSignedIn: () => void }) {
  const [token, setToken] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  return (
    <div className="flex min-h-dvh items-center justify-center bg-bg px-4">
      <form
        className="w-full max-w-[420px]"
        onSubmit={async (event) => {
          event.preventDefault();
          setPending(true);
          setError(null);
          try {
            await client.signIn(token.trim());
            setToken('');
            onSignedIn();
          } catch (failure) {
            setError(failure instanceof Error ? failure.message : 'Sign-in failed.');
          } finally {
            setPending(false);
          }
        }}
      >
        <p className="font-display inline-flex items-baseline text-[26px] leading-none lowercase">
          deku
          <span
            aria-hidden="true"
            className="ml-[3px] inline-block size-[7px] rounded-full bg-accent"
          />
        </p>
        <h1 className="mt-6 text-[17px] font-semibold">Sign in to this host</h1>
        <p className="mt-1 text-ink-2">
          Paste the dashboard token. This browser keeps a session for it; the token itself is not
          stored.
        </p>
        <Field label="Dashboard token" error={error} className="mt-5">
          <Input
            mono
            type="password"
            autoFocus
            autoComplete="off"
            value={token}
            onChange={(event) => setToken(event.target.value)}
            placeholder="dku_…"
          />
        </Field>
        <Button
          type="submit"
          variant="primary"
          className="mt-4 w-full"
          disabled={!token.trim() || pending}
        >
          {pending ? 'Signing in…' : 'Sign in'}
        </Button>
        <p className="mt-6 text-[12.5px] text-ink-3">
          Lost the token? Make a new one on the server:
        </p>
        <Cli className="mt-2" command="deku dashboard reset-token" />
      </form>
    </div>
  );
}
