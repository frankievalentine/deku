# Deku docs interface review

Change-scoped review of the docs interface pass described in [DESIGN.md](./DESIGN.md). Every
finding cites the source that produced it; the fix column names where the change landed.

## Scope and Coverage

Mode: `full`. Scope: the Starlight docs site in `docs/` (shell, header, navigation, article
typography, home page, build output). Stack: Astro 7.3.3 + Starlight 0.42.1 +
`@pelagornis/page` 1.2.5, local overrides in `src/components/starlight/` and
`src/styles/page-theme.css`. Project convention documents found: `AGENTS.md`,
`CONTRIBUTING.md` (docs verification commands), `docs/DESIGN.md` (added by this pass).
Boundary: the marketing-free dashboard app is out of scope; only the docs site was inspected.

| Domain | Evidence inspected | Result |
| --- | --- | --- |
| Accessibility | `PageFrame.astro`, `Header.astro`, `MobileMenuToggle.astro`, `MobileMenuOverlay.astro`, `base.css`, built HTML/AX structure | 6 findings, all fixed |
| Layout | `TwoColumnContent.astro`, `layout.css`, `PageFrame.astro`, 1280x900 screenshot, built CSS | 2 findings, both fixed |
| Writing | `index.md`, sidebar labels, 404 page copy | 1 finding, fixed |
| Typography | `page-theme.css`, `Hero.astro`, built CSS, 375px and 1280px screenshots | 2 findings, fixed |
| Colors | `theme.css`, `utilities.css`, measured link colours | 1 finding, fixed |
| UI polish | `Hero.astro`, `Button.astro`, `MobileMenuToggle.astro` states | 1 finding, fixed |

## Findings

| # | Severity | Domain | Status | Location | Before | After | Why |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | HIGH | Accessibility | Fixed | `Header.astro:579` (`@pelagornis/page`), no `logo` in `astro.config.mjs` | `@media (max-width:768px) { .page-title-text { display: none } }` leaves `<a href="/" class="page-site-title">` with no content | Wordmark stays visible at 1rem with `min-height: 44px` (`page-theme.css`) | The home link was blank and had no accessible name on mobile |
| 2 | HIGH | Accessibility | Fixed | `MobileMenuToggle.astro:87-118`, `MobileMenuOverlay.astro:348-371` | Toggle only flips `display` and a class: focus stays on the trigger, Escape does nothing, the overlay has no role | `PageFrame.astro` script adds `aria-haspopup="dialog"`, `aria-controls`, `aria-expanded`, `role="dialog"`, `aria-modal`, moves focus in, traps Tab, closes on Escape, restores the trigger | A menu a keyboard user cannot leave or dismiss is a blocker |
| 3 | HIGH | Accessibility | Fixed | `dist/index.html` before this pass: 0 matches for `id="_top"` | Starlight's skip link targets `#_top`; splash pages render the hero instead of `<h1 id="_top">`, so the link went nowhere | `PageFrame.astro` puts `id="_top"` on `<main class="main-frame">` on hero pages only | Skip targets must exist on every route |
| 4 | HIGH | Layout | Fixed | `TwoColumnContent.astro:31-40`, `layout.css:91-98`, 1280x900 screenshot | Nav rail 320 + right rail 320 + 5rem gutters leave the article ~433px, so `1. Get Dashboard Access Details` wrapped | Rails 240/200, article gutter 1rem, right rail hidden below 1440px | The primary reading column was the narrowest element on the page |
| 5 | MEDIUM | Typography | Fixed | `layout.css:91` (`.page-content-wrapper` has `max-width: none`) | Body measured ~40 characters per line | `html .page-content-wrapper { max-width: 72ch }` | 65-75ch is the readable measure for long-form text |
| 6 | MEDIUM | Accessibility | Fixed | `utilities.css:139-143` | `[data-theme="dark"] a:not(...) { color: var(--page-text) !important }` made body links identical to body text, with no underline | Content links keep `var(--sl-color-text-accent)` and carry a persistent underline | Link state was carried by nothing at all in dark mode |
| 7 | MEDIUM | Accessibility | Fixed | `Header.astro:587-599` | Mobile header controls sized 32x32px | 44x44px for search, theme, GitHub and menu controls | WCAG 2.5.8 plus touch ergonomics |
| 8 | MEDIUM | Accessibility | Fixed | `base.css:8-10`, `Sidebar.astro:21` | `scroll-behavior: smooth` and entrance animations ran regardless of user preference | `@media (prefers-reduced-motion: reduce)` block in `page-theme.css` | Motion must be opt-in |
| 9 | MEDIUM | Accessibility | Fixed | `page-theme.css`, theme `:focus-visible` rules limited to a few buttons | Chrome and content controls had no consistent visible focus ring | Global `:where(a, button, summary, [tabindex]):focus-visible` outline | Keyboard users could not see where they were |
| 10 | MEDIUM | Accessibility (build hygiene) | Fixed | `@pelagornis/page` `components.css`/`layout.css`/`utilities.css`, `refineui-system-icons.css` | 108 invalid `:global()` selectors and 6 unresolved font URLs produced 115 build warnings | PostCSS fixups in `astro.config.mjs` strip the inert rules and unpublished `.ttf`/`.otf` sources; real font files staged in `public/fonts` | Warning noise hid real problems and shipped invalid CSS |
| 11 | MEDIUM | Writing | Fixed | `src/content/docs/index.md:8-43` | Three filled primary hero buttons and three overlapping sections repeating the same links | One primary action (`Install Deku`), one secondary (`Get Started`), then Start Here / Common Guides / What You Can Do | Competing primaries and duplicate lists slow the first decision |
| 12 | LOW | UI polish | Fixed | `Hero.astro:93-107, 265-278` | Hero padding up to `5rem` plus header compensation, `text-wrap` unmanaged | Tighter padding, balanced title, `text-wrap: pretty` tagline capped at 58ch | Large empty bands above the fold |
| 13 | LOW | Typography | Fixed | `Hero.astro:178-185` | Tagline at `opacity: 0.75` over the gradient | Opacity removed, token colour kept | Reduced contrast margin for no visual gain |
| 14 | LOW | Accessibility | Fixed | `dist/` asset check | `/favicon.svg` requested by every page but absent from `public/` | `public/favicon.svg` added | Broken asset request on every route |
| 15 | MEDIUM | Accessibility | Regression fixed | `PageFrame.astro:22`, `TwoColumnContent.astro:11`, `@astrojs/starlight/dist/components/Page.astro:90` | Three nested `<main>` landmarks wrapped the page, so landmark navigation listed the same content more than once | `PageFrame` content wrapper is a `<div class="main-frame">`; the two vendor-owned `<main>` elements remain (see Pre-existing) | One landmark per page is the rule; the page now exposes a single landmark chain |

Intended right-rail breakpoint: the table of contents is rendered from `1440px` up. Below that
(including `1280px`) the article takes the full column at 72ch, because at smaller widths the
right rail is the element that would squeeze the measure.

## Considered but Rejected

| Location | Candidate | Rejected because |
| --- | --- | --- |
| `layout.css:11` | Unwrap `:global(main[data-pagefind-body])` to a live selector so the rule "works" | Unwrapping sets `display: none !important` on the element that holds all article content; the rules are inert today and must stay inert |
| `Header.astro`, `Hero.astro` | Fork the theme components to own the header/hero outright | 700+ lines duplicated per component; the `PageFrame` override and CSS reach the same outcome with far less surface |
| Starlight i18n warning | Declare a stub `i18n` collection to silence the last warning | Adds a content surface with no content for a cosmetic log line |
| Header icon glyphs | Replace RefineUI font glyphs with inline SVG across the header | The icon font is the theme's established system and now resolves; changing it is unrelated churn |

## Pre-existing

| Severity | Domain | Location | Issue |
| --- | --- | --- | --- |
| LOW | Accessibility | `TwoColumnContent.astro:11` wrapping `Page.astro:90` | The theme's layout component and Starlight's page component both emit a `<main>`, so two nested landmarks remain. Un-nesting them means forking the theme's ~250-line layout component (markup plus its scoped layout styles); left alone deliberately. |
| LOW | Accessibility | `SidebarSublist.astro:32` | Sidebar group labels are `<h4>` with no `<h3>` above them, so the heading outline starts at level 4. The component is not overridable without forking it; left alone. |
| LOW | Accessibility (build) | `@astrojs/starlight/dist/utils/translations.js:12` | One build warning remains: Starlight looks up its optional `i18n` collection on every render. Not CSS or fonts, and not fixable from site config without shipping a stub collection. |

## Verification

Run in `docs/`:

1. `bun install --frozen-lockfile` - `Checked 428 installs across 507 packages (no changes)`, exit 0.
2. `bun run check` - `6 files`, 0 errors, 0 warnings, 0 hints.
3. `bun run build` - exit 0, `13 page(s) built`, `[starlight:pagefind] Found 13 HTML files`, pagefind index reports `page_count: 12` (matching production search results), warnings down from 115 to 1 (the Starlight i18n lookup above).
4. `node scripts/verify-css-fixups.mjs` - exit 0: 54 inert `:global()` declarations across 43 rules stripped, the theme's Google Fonts imports kept (`dist` holds 3 statements, 2 unique urls), RefineUI `.woff2` faces kept, no `.ttf`/`.otf` reference left in `dist`.
5. A/B build comparison with the PostCSS fixup disabled: `8646` rules before, `8603` after, `43` rule keys removed, `0` non-`:global` rules removed, `0` declaration changes in surviving rules, `0` selectors added.
6. Artifact URL scan of `dist`: 13 HTML files, 21 unique root-relative references, 0 missing; 6 CSS `url()` references, 0 missing.
7. Shipped mobile-menu script evaluated against a DOM harness extracted from `dist/installation/index.html`: 10 checks pass (dialog semantics, collapsed initial state, focus moved in on open, Tab wrap, Escape closes, `aria-expanded` toggling, focus restored to the trigger even when the browser has already blurred to `<body>`).
8. `bun audit` - `No vulnerabilities found (checked 485 packages)`.
9. Statement at-rules confirmed intact in the emitted bundle: three Google Fonts `@import` statements (2 unique urls) and both `@layer` blocks survive; only block at-rules emptied by the removals can be dropped, and none were.

Not verified here: rendering in a real browser. This sandbox has no browser automation and blocks
localhost connections, so final column measurements need the root agent's browser pass. The root
agent's browser pass confirmed the mobile menu behaviour on the built site (focus entry, focus
trap, Escape, focus restoration).

## Verdict

`Approve` for this change scope: no `Introduced` or `Regression` finding remains open. The two
pre-existing items above are recorded but not this change's responsibility.
