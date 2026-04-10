import { useEffect, useState } from 'react';
import { getToken, TOKEN_CHANGE_EVENT } from '../lib/api';

export type TokenAccessState = 'unknown' | 'connected' | 'locked';

function readInitialTokenAccess(): TokenAccessState {
  if (typeof document !== 'undefined') {
    return document.documentElement.dataset.dashboardTokenState === 'connected'
      ? 'connected'
      : 'locked';
  }

  if (typeof window !== 'undefined') {
    return getToken() ? 'connected' : 'locked';
  }

  return 'locked';
}

export function useTokenAccess(): TokenAccessState {
  const [tokenAccess, setTokenAccess] = useState<TokenAccessState>(readInitialTokenAccess);

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
