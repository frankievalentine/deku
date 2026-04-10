import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import pagePlugin from '@pelagornis/page';

export default defineConfig({
  site: 'https://deku.vercel.app',
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
        { icon: 'github', label: 'GitHub', href: 'https://github.com/your-org/deku' },
      ],
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { label: 'Introduction', link: '/introduction/' },
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
          label: 'Operations',
          items: [
            { label: 'Agent Operations', link: '/agent-operations/' },
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
  ],
});
