import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const docsRoot = fileURLToPath(new URL('..', import.meta.url));
const vendorStyles = join(docsRoot, 'node_modules/@pelagornis/page/styles');
const iconFontCss = join(
  docsRoot,
  'node_modules/@refineui/web-icons/dist/fonts/refineui-system-icons.css',
);
const astroDist = join(docsRoot, 'dist/_astro');

const readDirCss = (dir) =>
  readdirSync(dir)
    .filter((name) => name.endsWith('.css'))
    .map((name) => readFileSync(join(dir, name), 'utf8'))
    .join('\n');

const themeCss = readDirCss(vendorStyles);
const distCss = readDirCss(astroDist);
const iconCss = readFileSync(iconFontCss, 'utf8');

const expectedGlobalRules = (themeCss.match(/:global\(/g) ?? []).length;
const globalSelectors = new Set(
  (themeCss.match(/[^{}]*:global\([^{}]*\{/g) ?? []).map((match) => match.replace(/\s+/g, ' ').trim()),
);
const googleUrlPattern = /https:\/\/fonts\.googleapis\.com\/[^"')]+/g;
const vendorGoogleUrls = new Set(themeCss.match(googleUrlPattern) ?? []);
const distGoogleUrls = new Set(distCss.match(googleUrlPattern) ?? []);
const droppedGoogleUrls = [...vendorGoogleUrls].filter((url) => !distGoogleUrls.has(url));
const vendorGoogleStatements = (themeCss.match(googleUrlPattern) ?? []).length;
const distGoogleStatements = (distCss.match(googleUrlPattern) ?? []).length;

const checks = [
  [
    'vendor css still contains the 54 inert :global() declarations',
    expectedGlobalRules === 54,
    expectedGlobalRules,
  ],
  [
    'inert :global() declarations sit on 43 rules',
    globalSelectors.size === 43,
    globalSelectors.size,
  ],
  ['no :global() selector survives into dist', !distCss.includes(':global(')],
  ['no .ttf/.otf font reference in dist', !/\.(?:ttf|otf)/.test(distCss)],
  [
    'RefineUI woff2 faces kept',
    (distCss.match(/refineui-system-icons-(?:regular|filled)\.woff2/g) ?? []).length >= 2,
  ],
  [
    'icon font css still lists the 4 unpublished formats',
    (iconCss.match(/\.(?:ttf|otf)(['"]?)\)/g) ?? []).length === 4,
  ],
  [
    'every google font import from the theme survives',
    vendorGoogleUrls.size > 0 && droppedGoogleUrls.length === 0,
    `vendor ${vendorGoogleStatements}, dist ${distGoogleStatements}, dropped ${droppedGoogleUrls.length}`,
  ],
  ['@layer blocks preserved', distCss.includes('@layer')],
  [
    'surviving theme selectors kept',
    distCss.includes('.page-content-wrapper') && distCss.includes('.page-toc-sticky'),
  ],
  ['re-expressed card rules present', distCss.includes('.sl-card')],
  ['local layout overrides present', distCss.includes('max-width:72ch!important')],
  [
    'mobile header touch targets present',
    distCss.includes('width:44px!important') && distCss.includes('--page-sidebar-width:240px'),
  ],
];

const failures = checks
  .filter(([, ok]) => !ok)
  .map(([label, , detail]) => `${label}${detail === undefined ? '' : ` (${detail})`}`);

if (failures.length > 0) {
  console.error(`css fixup regression:\n${failures.join('\n')}`);
  process.exit(1);
}

console.log(
  `css fixups ok: ${expectedGlobalRules} inert :global() occurrences on ${globalSelectors.size} rules stripped, ` +
    `${vendorGoogleStatements} theme google font import(s) kept (${distGoogleStatements} statements, ` +
    `${distGoogleUrls.size} unique urls in dist), fonts intact`,
);
