# Deku docs design contract

Extracted from the shipped docs site before this pass, so changes stay inside the existing
themed Starlight system instead of inventing a new one.

## Stack and ownership

| Piece | Version / location | Notes |
| --- | --- | --- |
| Astro | `7.3.3` (`docs/package.json`) | static output into `docs/dist` |
| Starlight | `0.42.1` | routing, sidebar data, search, TOC, content schema |
| `@pelagornis/page` | `1.2.5` | theme plugin: component overrides + CSS |
| Fonts | Rubik, IBM Plex Sans, IBM Plex Mono | loaded from Google Fonts in `src/styles/page-theme.css` |
| Local styles | `src/styles/page-theme.css` | only local stylesheet; loads **before** theme CSS |

The theme plugin registers its own component overrides (`Header`, `Footer`, `PageFrame`,
`Sidebar`, `TwoColumnContent`, `ContentPanel`, `Search`, `ThemeSelect`, `LanguageSelect`,
`SocialIcons`, `MarkdownContent`, `Hero`, `MobileMenuToggle`). A site override in
`astro.config.mjs` `components` always wins, so local overrides are additive and reversible.

## Tokens the pages actually use

- Surfaces and text: `--page-bg`, `--page-bg-secondary`, `--page-bg-elevated`,
  `--page-text`, `--page-text-secondary`, `--page-text-muted`, `--page-border`,
  `--page-accent`, `--page-accent-light`.
- Spacing: `--page-space-1 .. --page-space-32` (`0.25rem` steps up to `8rem`).
- Type: `--page-font-family`, `--page-font-size-xs .. 6xl`, `--page-line-height-tight/normal`,
  `--page-radius-sm .. 3xl`, `--page-shadow-xs .. 2xl`.
- Starlight accents: `--sl-color-accent` (`#3d50f5` light / `#3369ff` dark),
  `--sl-color-text-accent` (`#3d50f5` light / `#b3c7ff` dark), `--sl-color-accent-low/high`.
- Fonts are re-pointed locally: headings `Rubik`, body `IBM Plex Sans`, code `IBM Plex Mono`.

`public/fonts/*.woff2` are copied from `@refineui/web-icons@0.3.32` (MIT, author Pelagornis) so the
theme's icon font URLs resolve; see `public/fonts/LICENSE-refineui-web-icons.txt` for the notice.

## Layout system (as measured before this pass)

- Header: fixed `--page-header-height` (`4rem`, `3.5rem` under `640px`).
- Shell (`src/components/starlight/PageFrame.astro`): `.page` (100vh column) with a
  non-scrolling header and a `.page-scroll-wrap` that owns page scrolling.
- Desktop nav rail: `--page-sidebar-width: 320px` (`position: fixed` at `>= 769px`).
- Reading column: `.page-content-wrapper`, `margin-left` = sidebar width,
  `margin-right: 5rem`, `flex: 1`, **no measure cap**.
- Right rail: `--page-toc-width: 320px` + `--page-toc-right-margin: 40px`; in flow at
  `>= 1280px`, hidden below that.
- Breakpoints in play: `640/641`, `768/769`, `1024/1025`, `1280`, `1440`.

Measured at `1280x900` before this pass: nav rail `320`, article `~433` (`~40ch`), right
rail `~308`. H2 `1. Get Dashboard Access Details` wrapped, and body text broke near 40
characters per line.

## Improvement contract for this pass

Goal: keep every route, anchor, search result and code sample, and make the docs readable
and operable on narrow and wide screens.

1. **Reading measure 65-75ch.** Cap the article column at `72ch`; keep code frames inside
   the column with their own horizontal scrolling.
2. **Column balance.** Desktop nav rail `240px`, right rail `200px`, tighter article gutter,
   so the article reaches measure at `1280px` without squeezing. The right rail renders from
   `1440px` up; below that the article takes the full column at 72ch.
3. **Home hero.** Keep the existing gradient/radius language; cut vertical padding, cap the
   tagline measure, drop the redundant third hero button.
4. **Mobile header.** Keep a visible, named home link (wordmark) instead of a blank `<a>`;
   comfortable touch targets for the header controls.
5. **Mobile navigation.** Replace the class-toggled overlay with a native `<dialog>` menu:
   labelled trigger with `aria-expanded`, focus moves in on open, `Escape` closes, focus
   returns to the trigger.
6. **Accessibility floor.** Visible `:focus-visible` rings, `prefers-reduced-motion` support,
   a working skip-link target on the splash page, and links that are not signalled by color
   alone.
7. **Build hygiene.** No invalid `:global()` CSS in the emitted bundle and no unresolved
   font asset URLs, without editing `node_modules` and without changing the pinned versions.

## Out of scope

- No new brand, palette, font, or component library.
- No change to content facts, command surfaces, or route structure.
- No `@pelagornis/page` version change, no fork of the theme's component tree beyond
  `PageFrame` and `MobileMenuToggle`, no plugin authoring, no deploy.
