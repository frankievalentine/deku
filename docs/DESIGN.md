# Deku site design

The site has two surfaces that share one set of tokens:

| Surface | Source | Notes |
| --- | --- | --- |
| Landing page | `src/pages/index.astro`, `src/styles/landing.css` | Standalone Astro page at `/`. No Starlight layout. |
| Docs | Starlight, `src/styles/docs.css` | Stock Starlight, themed through its CSS variables. No theme plugin, no component overrides. |
| Tokens | `src/styles/tokens.css` | Colours and fonts for both. Change values here, not in the surface files. |

## Direction

An operator's spec sheet. Ink on warm paper over a faint drafting grid, hairline rules instead of
boxes, mono labels for anything the CLI prints. Warm neutrals use hue 95, matching the dashboard.
One signal colour (vermilion) marks live state, the current page, and primary actions; nothing
else is coloured.

- Display: Archivo, using its width axis (`font-stretch` 104-125%) for headings and the wordmark.
- Body: IBM Plex Sans. Code and labels: IBM Plex Mono.
- Both themes come from the same token names; `tokens.css` switches the values on
  `data-theme` (set by Starlight, and read from `starlight-theme` on the landing page) with the
  system preference as the fallback.

## Layout rules

- The landing page puts every section in one centred `.frame` (1240px, growing to 1440px on very
  wide screens), so the composition holds from 390px to 2560px.
- The docs keep Starlight's layout. `--sl-content-width` sets the reading measure; Starlight
  centres the column beside the sidebar and shows the table of contents on wide screens.
- Docs URLs stay at the site root (`/installation/`, `/reference/cli-reference/`), because the
  README, the installer, and external guides link to them.

## Verification

Run in `docs/`:

```bash
bun run check
bun run build
```

Then check `/` and one docs page at 390px, 1280px and 2560px in both themes. The landing page must
not scroll horizontally at 390px.
