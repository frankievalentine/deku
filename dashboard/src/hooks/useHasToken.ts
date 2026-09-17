import { useEffect, useState } from 'react';
import { getToken, TOKEN_CHANGE_EVENT } from '../lib/api';

export type TokenAccessState = 'unknown' | 'connected' | 'locked';

/**
 * First render is deliberately browser-independent. Reading the token here
 * would let an authenticated client hydrate a different tree than the server
 * rendered (ConnectScreen vs the app list), which React reports as a
 * hydration mismatch. The token is resolved in the effect below.
 */
export function useTokenAccess(): TokenAccessState {
  const [tokenAccess, setTokenAccess] = useState<TokenAccessState>('unknown');

  useEffect(() => {
    function syncToken() {
      const nextTokenAccess = getToken() ? 'connected' : 'locked';
      document.documentElement.dataset.dashboardTokenState = nextTokenAccess;
      setTokenAccess(nextTokenAccess);
    }

    syncToken();
    window.addEventListener(TOKEN_CHANGE_EVENT, syncToken);
    window.addEventListener('storage', syncToken);

    return () => {
      window.removeEventListener(TOKEN_CHANGE_EVENT, syncToken);
      window.removeEventListener('storage', syncToken);
    };
  }, []);

  return tokenAccess;
}

export function useHasToken(): boolean {
  return useTokenAccess() === 'connected';
}
