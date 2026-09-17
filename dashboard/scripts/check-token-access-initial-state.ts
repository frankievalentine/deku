import { createElement } from 'react';
import { renderToString } from 'react-dom/server';
import { useTokenAccess } from '../src/hooks/useHasToken';
import { getToken } from '../src/lib/api';

interface FakeScope {
  window?: { localStorage: { getItem(key: string): string | null } };
  document?: { documentElement: { dataset: Record<string, string> } };
}

const scope = globalThis as unknown as FakeScope;
let failures = 0;

function check(name: string, condition: boolean): void {
  if (condition) {
    console.log(`PASS  ${name}`);
    return;
  }

  failures += 1;
  console.error(`FAIL  ${name}`);
}

/**
 * Effects never run inside `renderToString`, so each render below is the
 * pre-mount render React performs while hydrating: the exact tree that has to
 * match the server output.
 */
function renderTokenAccessProbe(): string {
  function TokenAccessProbe() {
    return createElement('span', { 'data-token-access': useTokenAccess() });
  }

  return renderToString(createElement(TokenAccessProbe));
}

function installBrowser(token: string | null, tokenState: 'connected' | 'locked'): void {
  scope.window = { localStorage: { getItem: (key) => (key === 'deku_token' ? token : null) } };
  scope.document = { documentElement: { dataset: { dashboardTokenState: tokenState } } };
}

function removeBrowser(): void {
  delete scope.window;
  delete scope.document;
}

removeBrowser();
const serverMarkup = renderTokenAccessProbe();
check(
  'server render (no browser globals) renders the unknown state',
  serverMarkup === '<span data-token-access="unknown"></span>'
);

installBrowser('dku_test_token', 'connected');
check(
  'authenticated client: pre-hydration render matches the server markup',
  renderTokenAccessProbe() === serverMarkup
);
check(
  'authenticated client: mounted effect resolves the token to connected',
  getToken() === 'dku_test_token'
);

installBrowser(null, 'locked');
check(
  'anonymous client: pre-hydration render matches the server markup',
  renderTokenAccessProbe() === serverMarkup
);
check('anonymous client: mounted effect stays locked', getToken() === null);

removeBrowser();

if (failures > 0) {
  console.error(`token access hydration fixture: ${failures} check(s) failed`);
  process.exit(1);
}

console.log('token access hydration fixture: all checks passed');
