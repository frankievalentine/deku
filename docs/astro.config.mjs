import { defineConfig } from 'astro/config';
import mdx from '@astrojs/mdx';
import starlight from '@astrojs/starlight';
export default defineConfig({
  site: 'https://get-deku.vercel.app',
  integrations: [
    starlight({
      disable404Route: true,
      title: 'Deku',
      description: 'A modern, lightweight self-hosted PaaS built with Rust, Angie, and Astro.',
      customCss: ['./src/styles/tokens.css', './src/styles/docs.css'],
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
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'CLI Reference', link: '/reference/cli-reference/' },
            { label: 'API Reference', link: '/reference/api-reference/' },
            { label: 'deku.toml', link: '/reference/deku-toml/' },
          ],
        },
        {
          label: 'Workflows',
          items: [
            { label: 'App templates', link: '/app-templates/' },
            { label: 'Environments', link: '/environments/' },
            { label: 'Automatic certificates', link: '/certificates/' },
            { label: 'App authentication', link: '/app-authentication/' },
            { label: 'Build server', link: '/build-server/' },
            { label: 'AGENTS.md', link: '/agents/' },
          ],
        },
        {
          label: 'Operations',
          items: [
            { label: 'Production checklist', link: '/production/' },
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
            { label: 'Architecture', link: '/architecture/' },
            { label: 'Contributing', link: '/contributing/' },
          ],
        },
      ],
    }),
    mdx(),
  ],
});
