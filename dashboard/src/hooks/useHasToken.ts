import { useEffect, useState } from 'react';
import { getToken, TOKEN_CHANGE_EVENT } from '../lib/api';

export type TokenAccessState = 'unknown' | 'connected' | 'locked';

export function useTokenAccess(): TokenAccessState {
  const [tokenAccess, setTokenAccess] = useState<TokenAccessState>('unknown');

  useEffect(() => {
    function syncToken() {
      setTokenAccess(getToken() ? 'connected' : 'locked');
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
