import { sanitizeToastConfig } from '../src/lib/basecoat-compat';

const BASE_URL = 'https://dashboard.test';
let failures = 0;

function check(name: string, condition: boolean): void {
  if (condition) {
    console.log(`PASS  ${name}`);
    return;
  }

  failures += 1;
  console.error(`FAIL  ${name}`);
}

const markup = '<img src=x onerror="alert(1)">';
const markupToast = sanitizeToastConfig({ title: markup, description: `App ${markup}` }, BASE_URL);

check(
  'title markup is escaped',
  markupToast.title === '&lt;img src=x onerror=&quot;alert(1)&quot;&gt;'
);
check('title contains no executable markup', !markupToast.title?.includes('<'));
check('description markup is escaped', !markupToast.description?.includes('<'));

const scriptToast = sanitizeToastConfig({ description: '<script>alert(1)</script>' }, BASE_URL);
check('script tags cannot survive escaping', !scriptToast.description?.includes('<script'));

const javascriptHref = sanitizeToastConfig(
  { action: { label: 'Retry', href: 'javascript:alert(1)' } },
  BASE_URL
);
check('javascript: hrefs are dropped', javascriptHref.action?.href === undefined);

const dataHref = sanitizeToastConfig(
  { action: { label: 'Retry', href: 'data:text/html,<script>alert(1)</script>' } },
  BASE_URL
);
check('data: hrefs are dropped', dataHref.action?.href === undefined);

const relativeHref = sanitizeToastConfig(
  { action: { label: 'Open settings', href: '/settings' } },
  BASE_URL
);
check('relative hrefs are kept', relativeHref.action?.href === '/settings');

const absoluteHref = sanitizeToastConfig(
  { action: { label: 'Docs', href: 'https://docs.example.com' } },
  BASE_URL
);
check('https hrefs are kept', absoluteHref.action?.href === 'https://docs.example.com');

const quotedHref = sanitizeToastConfig(
  { action: { label: 'Docs', href: 'https://example.test/" onmouseover="alert(1)' } },
  BASE_URL
);
check(
  'attribute-breaking quotes in hrefs are escaped',
  quotedHref.action?.href === 'https://example.test/&quot; onmouseover=&quot;alert(1)'
);
check('escaped href keeps no raw double quote', !quotedHref.action?.href?.includes('"'));

const quotedRelativeHref = sanitizeToastConfig(
  { action: { label: 'Settings', href: '/settings?x="><script>alert(1)</script>' } },
  BASE_URL
);
check(
  'relative hrefs cannot break the attribute or open a tag',
  !quotedRelativeHref.action?.href?.includes('"') &&
    !quotedRelativeHref.action?.href?.includes('<') &&
    quotedRelativeHref.action?.href?.includes('&quot;') === true
);

const handlerToast = sanitizeToastConfig(
  { title: 'Saved', onclick: 'alert(1)', icon: '<img src=x>' },
  BASE_URL
);
check('inline handlers are removed', !Object.hasOwn(handlerToast, 'onclick'));
check('icon markup is removed', !Object.hasOwn(handlerToast, 'icon'));

const durationToast = sanitizeToastConfig({ duration: -1 }, BASE_URL);
check('persistent duration -1 is preserved', durationToast.duration === -1);
check(
  'non-numeric duration is dropped',
  sanitizeToastConfig({ duration: '-1' }, BASE_URL).duration === undefined
);
check(
  'unknown category falls back to info',
  sanitizeToastConfig({ category: 'x' }, BASE_URL).category === 'info'
);

const cancelToast = sanitizeToastConfig({ cancel: { label: '<b>Dismiss</b>' } }, BASE_URL);
check('cancel label is escaped', cancelToast.cancel?.label === '&lt;b&gt;Dismiss&lt;/b&gt;');

if (failures > 0) {
  console.error(`${failures} basecoat toast safety check(s) failed`);
  process.exit(1);
}

console.log('basecoat toast safety: all checks passed');
