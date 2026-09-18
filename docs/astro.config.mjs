import { defineConfig } from 'astro/config';
import mdx from '@astrojs/mdx';
import starlight from '@astrojs/starlight';
import pagePlugin from '@pelagornis/page';

function splitTopLevelCommas(value) {
  const parts = [];
  let depth = 0;
  let current = '';

  for (const char of value) {
    if (char === '(') depth += 1;
    else if (char === ')') depth = Math.max(0, depth - 1);

    if (char === ',' && depth === 0) {
      parts.push(current);
      current = '';
      continue;
    }

    current += char;
  }

  parts.push(current);
  return parts;
}

const PUBLISHED_FONT_FORMATS = /\.(?:ttf|otf)(['"]?)\)/;
const ICON_FONT_FAMILY = 'RefineUI-System-Icons';
const THEME_PLAIN_CSS = /[/\\]@pelagornis[/\\]page[/\\]styles[/\\][^/\\]+\.css$/;
const ICON_FONT_CSS = /[/\\]@refineui[/\\]web-icons[/\\]dist[/\\]fonts[/\\][^/\\]+\.css$/;

function sourceFile(node) {
  return node.source?.input?.file;
}

/**
 * Two upstream packaging defects, fixed at the CSS level so the pinned theme versions stay put:
 *
 * 1. `@pelagornis/page` ships plain CSS files that still use Astro's `:global()` wrapper, which is
 *    only valid inside an Astro `<style>` block. Browsers drop those selectors, so the 54 rules are
 *    already inert; lightningcss warns about every one while minifying. The subset worth keeping is
 *    re-expressed with real selectors in `src/styles/page-theme.css`.
 * 2. `refineui-system-icons.css` lists `.ttf`/`.otf` fallbacks that the package never publishes, so
 *    Vite warns once per missing file. The shipped `.woff2`/`.woff` faces are left alone.
 */
function docsCssFixups() {
  return {
    postcssPlugin: 'deku-docs-css-fixups',
    OnceExit(root) {
      root.walkRules((rule) => {
        if (!rule.selector.includes(':global(')) return;

        const file = sourceFile(rule);
        if (!file || !THEME_PLAIN_CSS.test(file)) return;

        rule.remove();
      });

      root.walkAtRules('font-face', (atRule) => {
        const file = sourceFile(atRule);
        if (!file || !ICON_FONT_CSS.test(file)) return;

        const family = atRule.nodes?.find(
          (node) => node.type === 'decl' && node.prop === 'font-family',
        );

        if (!family?.value.includes(ICON_FONT_FAMILY)) return;

        atRule.walkDecls('src', (declaration) => {
          if (!PUBLISHED_FONT_FORMATS.test(declaration.value)) return;

          const kept = splitTopLevelCommas(declaration.value)
            .filter((part) => !PUBLISHED_FONT_FORMATS.test(part))
            .map((part) => part.trim())
            .filter(Boolean);

          if (kept.length > 0) declaration.value = kept.join(', ');
        });
      });

      root.walkAtRules((atRule) => {
        if (atRule.name === 'keyframes') return;
        // Statement at-rules (@import, @charset, `@layer a, b;`) have no `nodes` array and must
        // survive; only block at-rules emptied by the removals above are dropped.
        if (!Array.isArray(atRule.nodes)) return;
        if (atRule.nodes.length === 0) atRule.remove();
      });
    },
  };
}

export default defineConfig({
  site: 'https://get-deku.vercel.app',
  integrations: [
    starlight({
      plugins: [pagePlugin()],
      components: {
        PageFrame: './src/components/starlight/PageFrame.astro',
      },
      disable404Route: true,
      title: 'DEKU Docs',
      description: 'A modern, lightweight self-hosted PaaS built with Rust, Angie, and Astro.',
      customCss: ['./src/styles/page-theme.css'],
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/frankievalentine/deku' },
      ],
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { label: 'Installation', link: '/installation/' },
            { label: 'Get Started', link: '/get-started/' },
            { label: 'Dashboard Overview', link: '/dashboard-overview/' },
            { label: 'Architecture', link: '/architecture/' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'CLI Reference', link: '/reference/cli-reference/' },
            { label: 'API Reference', link: '/reference/api-reference/' },
            { label: 'deku.toml', link: '/reference/deku-toml/' },
            { label: 'Plugin API', link: '/reference/plugin-api/' },
          ],
        },
        {
          label: 'Workflows',
          items: [
            { label: 'App templates', link: '/app-templates/' },
            { label: 'App authentication', link: '/app-authentication/' },
            { label: 'Build server', link: '/build-server/' },
            { label: 'AGENTS.md', link: '/agents/' },
          ],
        },
        {
          label: 'Operations',
          items: [
            { label: 'Traffic control', link: '/traffic-control/' },
            { label: 'Deploy tokens', link: '/deploy-tokens/' },
            { label: 'Lifecycle hooks', link: '/hooks/' },
            { label: 'Runtime access', link: '/runtime-access/' },
            { label: 'Resource limits', link: '/resource-limits/' },
            { label: 'Backups', link: '/backups/' },
            { label: 'Diagnostics', link: '/diagnostics/' },
            { label: 'Logs', link: '/logs/' },
            { label: 'Monitoring and alerts', link: '/monitoring/' },
          ],
        },
        {
          label: 'Project',
          items: [
            { label: 'Contributing', link: '/contributing/' },
          ],
        },
      ],
    }),
    mdx(),
  ],
  vite: {
    css: {
      postcss: { plugins: [docsCssFixups()] },
    },
  },
});
