# Deku Dashboard Design

The dashboard is a compact operational surface for self-hosted apps. Density,
scanability, and repeated action beat illustration. No marketing intros, no
decorative hero sections, no restating in prose what the data already shows.

Source of truth for values is `src/styles/global.css`. This file explains intent
and the shared contract; change tokens in the stylesheet, not here.

## Ownership

- **Shell surface** (this doc's direct scope): `src/layouts/Base.astro`,
  `src/layouts/Shell.astro`, `src/components/Header.astro`,
  `src/components/Sidebar.astro`, `src/components/ShellController.tsx`,
  `src/components/ConnectScreen.tsx`, `src/components/ConfirmModal.tsx`,
  `src/components/TableScroll.tsx`, `src/lib/shell.ts`, `src/lib/basecoat-compat.ts`,
  and the shell half of `src/styles/global.css`.
- **App pages** (owned separately): `AppList*`, `AppDetailPage*`, `AppDeployPanel`,
  `LogStream`, and feature pages. Reuse the shared classes below instead of
  introducing parallel styles.
- Adding a new shared class means adding it to `global.css` here, with a token
  for every value. Page-local composition belongs in the page component.

## Shared class contract

| Class | Purpose |
| --- | --- |
| `panel`, `hero-panel` | Framed content surface. `hero-panel` is the top-of-page summary band. |
| `panel-grid`, `panel-span-full` | Two-column panel grid. `panel-span-full` on a child makes it span the full row, e.g. a wide table. |
| `page-header`, `page-title`, `page-copy` | Page heading row. `page-title` is the only display-scale type. |
| `section-title`, `eyebrow` | Section heading and small uppercase label. |
| `metrics-grid`, `metric-card` | Numeric summary tiles; 4-up desktop, 2x2 below 1200px. |
| `status-band`, `status-band-item` | Single-row figures inside `hero-panel`; divider-separated, not boxed. Use instead of `metrics-grid` when the page already has a hero. |
| `alert-list`, `alert-row` | Stacked alert rows with a severity edge; for narrow columns where an alert table would clip. |
| `data-grid` | Label/value pairs. |
| `stack-sm/md/lg`, `cluster`, `button-row`, `form-actions` | Spacing and grouping primitives. |
| `table`, `table-scroll` | Data tables; always wrap in `TableScroll` for narrow viewports. |
| `btn`, `btn-primary`, `btn-secondary`, `btn-outline`, `btn-ghost`, `btn-danger` | Buttons. Size with `btn-sm` (Basecoat 1.0 compat alias). |
| `input`, `textarea`, `form-group`, `form-label` | Form controls and labels. |
| `tabs`, `app-tabs`, `app-tab`, `app-tabs-track` | Basecoat tabs root plus the app's tab-button treatment. |
| `console-output`, `console-line` | Streaming command output in the app console. |
| `empty-state`, `loading-state`, `error-state` | Page-level states. |
| `modal-shell`, `modal-badge`, `modal-title`, `modal-copy` | Confirmation dialog. The backdrop is the dialog's `::backdrop` pseudo-element, not a class. |

## Tokens

### Color

Semantic roles, both appearances. Values live in `global.css`.

| Role | Light | Dark |
| --- | --- | --- |
| `--background` | `oklch(0.985 0.002 95)` | `oklch(0.17 0.004 95)` |
| `--foreground` | `oklch(0.19 0.004 95)` | `oklch(0.97 0.002 95)` |
| `--card` / `--popover` | `oklch(1 0 0)` | `oklch(0.205 0.004 95)` |
| `--muted` / `--accent` | `oklch(0.965 0.002 95)` / `oklch(0.945 0.003 95)` | `oklch(0.255 0.004 95)` / `oklch(0.295 0.004 95)` |
| `--muted-foreground` | `oklch(0.54 0.01 95)` | `oklch(0.72 0.008 95)` |
| `--border` / `--input` / `--ring` | `oklch(0.91 0.002 95)` / same / `oklch(0.72 0.008 95)` | `oklch(1 0 0 / 0.12)` / `oklch(1 0 0 / 0.14)` / `oklch(0.56 0.006 95)` |
| `--destructive` | `oklch(0.63 0.23 26)` | `oklch(0.71 0.18 24)` |
| `--color-focus-ring` | `oklch(0.62 0.008 95)` | `oklch(0.72 0.008 95)` |
| `--color-control-border` | `oklch(0.65 0.006 95)` | `oklch(0.52 0.006 95)` |
| `--sidebar-item-hover-bg` | `color-mix(foreground 5%)` | `color-mix(foreground 8%)` |
| `--sidebar-item-active-bg` | `oklch(0.895 0.004 95)` | `oklch(0.315 0.004 95)` |
| `--sidebar-item-active-fg` | `oklch(0.16 0.004 95)` | `oklch(0.985 0.002 95)` |
| `--sidebar-item-active-soft` | `oklch(0.42 0.005 95)` | `oklch(0.8 0.006 95)` |
| `--sidebar-item-active-indicator` | `oklch(0.32 0.004 95)` | `oklch(0.93 0.002 95)` |
| `--command-item-active` | `oklch(0.9 0.003 95)` | `oklch(0.32 0.004 95)` |
| `--command-item-meta` | `oklch(0.47 0.008 95)` | `oklch(0.72 0.008 95)` |

Status and tone tokens: `--color-status-running` `#16a34a`,
`--color-status-stopped`/`--color-status-error` `#dc2626`, `--color-status-building`
`#ca8a04`, plus `--tone-{warning,success,danger}-{fg,bg,border}`. Status is never
carried by color alone: pair the dot or tint with a label or icon.

### Space, radius, type, motion

- Space: `--space-1..12` = 4, 8, 12, 16, 20, 24, 28, 32, 40, 48px.
- Radius: `--radius` 0.75rem; `--radius-sm` `calc(radius - 4px)`, `--radius-md`
  `calc(radius - 2px)`, `--radius-lg` `radius`, `--radius-pill` 9999px. Nested
  surfaces step down one level.
- Type: `--font-heading`/`--font-body` system sans, `--font-mono` system mono.
  Sizes come from tokens, not ad-hoc values: `--text-page-title` 28px
  (`h1`/`.page-title`, steps to `--text-display` 24px below 1200px),
  `--text-display` 24px (connect screen, `.connect-title`),
  `--text-section-title` 20px (`h2`/`.section-title`/`.modal-title`),
  `--text-card-title` 17px (`h3`/`.deploy-title`), `--text-metric` 22px
  (`.metric-card strong`), `--text-body` 15px,
  `--text-meta` 13px, `--text-label` 12px floor for uppercase labels.
  Headings `line-height` ~1.1, body 1.6, uppercase labels get positive
  letter-spacing. Tabular figures on changing numbers.
- Elevation: `--shadow-card` only. Borders carry structure and state.
- Chrome: `--sidebar-width` 17rem (mobile 19rem), `--header-height` 80.5px,
  `--header-control-height` 2.5rem for every header control. The header row is
  a single centred column and `.shell-header-actions` centres its children, so
  search, action and status controls share one explicit height instead of
  growing with their padding.
- Motion: `--transition-fast` 160ms ease, color/background/border/opacity only.
  Never `transition: all`. All non-essential motion sits inside
  `@media (prefers-reduced-motion: no-preference)` and has a static cue.

## Density target

- Desktop: 4-up metric tiles, 2-column panels, 20-24px panel padding.
- Below 1200px: metric tiles go 2x2 and `page-title` steps down.
- Below 1024px: single-column panels, sidebar becomes an overlay.
- Mobile: still 2x2 metrics; tighter padding, no horizontal scroll at 320px.
- Hero panels summarize state and actions. They never carry marketing copy.

## Accessibility floor

- First focusable element is "Skip to main content", targeting the single
  `<main id="main" tabindex="-1">`.
- `:focus-visible` gets a 2px ring using `--ring` with offset; never remove
  focus without a verified replacement. Forced-colors mode keeps system colors.
- Pointer targets are at least 44x44px on touch surfaces and 40x40px on desktop.
- Inputs render at 16px on mobile so iOS does not zoom.
- Contrast: body text meets 4.5:1, focus indicators and control borders meet
  3:1, in both appearances. Error surfaces use the `--tone-danger-*` tokens
  rather than inventing reds. Measured (oklch to sRGB, WCAG 2): light muted text
  4.84:1 on background and 5.06:1 on card, `--color-focus-ring` 3.49:1 and
  3.64:1, `--color-control-border` 3.23:1 against the input fill and 3.10:1
  against the page, error toast 6.13:1; dark muted text 7.71:1 and 7.22:1, focus
  ring 7.71:1 and 7.22:1, control border 3.25:1 and 3.47:1, error toast 10.46:1.
- Sidebar and command surfaces measured the same way: light current-page label
  14.18:1 and its description 6.18:1 on the filled row, nav description 4.91:1
  on the sidebar, focus ring 3.54:1 against the sidebar; light command label
  13.70:1 on the selected row, shortcut chip 6.82:1 on the popover and 5.06:1 on
  the selected row. Dark: current-page label 12.37:1, its description 6.92:1,
  nav description 7.22:1, command label 11.63:1, shortcut chip 7.22:1 on the
  popover and 5.12:1 on the selected row.
- `--color-control-border` is the boundary for `.input`, `.textarea`,
  `select.input`, and `.file-picker`. Decorative panel, divider, and table
  borders keep `--color-border`/`--color-border-strong` and are not held to 3:1.
- Live regions: `role="status"` on the toaster container; errors may use
  `role="alert"`. Toasts carrying an error or an action stay until dismissed.
- Modals trap focus, close on Escape, and return focus to the trigger.

## Interaction patterns

- **Toasts** (`showToast`): informational 5s auto-dismiss; errors and anything
  with an action persist until dismissed. Appears bottom-right, stacks upward.
- **Command palette**: ⌘K / Ctrl-K opens, focus moves to the input, Escape and
  the close button close it, and focus returns to the element that opened it.
  Arrow keys and Enter keep working; the item list stays filterable. The dialog
  is `min(34rem, 100vw - 2rem)` wide, sits at 8vh, and is capped at
  `min(32rem, 100dvh - 16vh)` so the list scrolls rather than running off-screen.
  The search row is a bordered field (`:focus-within` carries the ring, the raw
  input has no outline) with the close control inside it, rows are 44px tall
  with a right-aligned shortcut chip, and filtering hides rows and empty groups
  and falls back to `data-empty`. Basecoat rules live in `@layer components`, so
  every unlayered override here wins on layer order; state rules such as
  `[aria-hidden="true"] { display: none }` must be restated explicitly.
- **Sidebar**: Basecoat `.sidebar` with `data-side="left"` and
  `data-breakpoint`. Desktop navigation is permanent; below 1024px it becomes a
  modal overlay. Opening it on mobile moves focus into the panel, marks
  `.shell-body` inert, traps Tab, closes on Escape or the close button, and
  returns focus to the toggle. Layout sync and the toggle drive it through the
  element method API (`src/lib/sidebar-overlay.ts`). Closing on a control click
  is Basecoat's own behavior, not ours.
- **ConfirmModal**: required for destructive actions; confirm button repeats the
  consequence, never "OK". Closing is idempotent: the backdrop, Cancel and the
  native `close` event all route through one guarded notifier, so a caller never
  receives two close callbacks for one dismissal.
- **Navbar status pill**: `.health-indicator` shows `.health-dot` +
  `.health-label`, is 9.75rem wide for every state (Online, Offline, Awaiting
  token), and matches `.shell-action-button` at 2.6rem tall.

## Basecoat 1.0

- `global.css` imports `basecoat-css` then `basecoat-css/compat`; the compat
  layer supplies pre-1.0 aliases (`btn-sm`, `btn-sm-icon-outline`).
- New markup uses the 1.0 API: a root class plus documented attributes
  (`class="btn" data-variant="outline" data-size="sm"`).
- Tailwind preflight (in `@layer base`) owns the element reset. `global.css`
  must not restate `box-sizing`/`margin`/`padding` on `*`: unlayered rules
  outrank every `@layer components` rule and would strip padding from Basecoat
  components (select options, tabs, sidebar groups, dialogs).
- Basecoat 1.0 removed the document-level `basecoat:toast` and
  `basecoat:sidebar` events. `src/lib/basecoat-compat.ts` bridges the app's
  existing event dispatches onto the supported element methods
  (`toaster.toast()`, `sidebar.open()/close()/toggle()`).
- The component scripts are imported in `Base.astro` in dependency order after
  the runtime; `tabs` is imported alongside `select`, `popover`, `sidebar`,
  `command`, and `toast`. Each `basecoat-css/<name>` module needs a
  `declare module` entry in `src/basecoat.d.ts`.
- `SelectField` is a React wrapper over the documented `div.select` contract.
  Basecoat rescans options through `refresh()`, dispatches `change` with
  `detail.value`, and closes on outside clicks but not on window blur, so the
  wrapper adds the blur close.
- App detail tabs are Basecoat's `.tabs`: `<nav role="tablist">` of
  `role="tab"` buttons over `role="tabpanel"` panels. Basecoat owns keyboard
  navigation and selection on the DOM and emits no change event, so a
  `MutationObserver` mirrors the selected tab into React state and the hash.
  Panels stay mounted as stable `role="tabpanel"` wrappers toggled with
  `hidden`, but each panel's content only mounts while it is active, so the log
  stream and panel queries stay lazy.
- The sidebar keeps its visual design on Basecoat's markup: `data-side="left"`,
  `<nav>`, and `role="group"` groups labeled by a heading. Basecoat's sidebar
  script handles the mobile close-on-any-control-click; `sidebar-overlay.ts`
  keeps only the modal focus trap, body `inert`, Escape, and focus return.

## Object store providers

The daemon has no provider registry: `provider` is a stored label and the only
behavior it drives is a region default and path-vs-virtual addressing, with
signing always the generic S3 SigV4 path. `src/lib/object-store-providers.ts`
holds the presets (R2, S3, B2, MinIO, Wasabi, DigitalOcean Spaces, Other) that
fill those conventions. Keep two invariants when editing it:

- **Write the canonical slug** (`r2`, `aws`, `b2`, `minio`, `wasabi`,
  `digitalocean`, `custom`) as the stored `provider`, so existing configs and
  `deku objectstore info` stay consistent.
- **Never rewrite a saved value by opening the page.** A stored provider outside
  the preset list gets its own "(saved)" option instead of being normalized to
  `custom`. Every field stays editable, including address style, so MinIO and
  unlisted services remain usable.

## Copy

Sentence case everywhere, verb-first buttons ("Create app", "Rotate token"),
errors that state the fix next to the field that failed, and no exclamation
marks or "oops". Link text names its destination. Prefer the word an operator
would use over the internal API term: "Domains" not "Proxy", "Processes" not
"Runtime", "Databases and caches" not "Managed services", "Deploy tokens",
"health check", and "environment" only when the operator needs it. Say what a
control does and what happens next, not the mechanism behind it ("Runs a
one-off command in a fresh copy of the app image", not "console exec").
