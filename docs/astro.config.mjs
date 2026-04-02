import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  integrations: [
    starlight({
      title: 'Deku',
      description: 'A modern, lightweight self-hosted PaaS',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/your-org/deku' },
      ],
      sidebar: [
        { label: 'Getting Started', items: [
          { label: 'Introduction', link: '/docs/introduction/' },
          { label: 'Installation', link: '/docs/installation/' },
          { label: 'Quick Start', link: '/docs/quickstart/' },
        ]},
        { label: 'Reference', items: [
          { label: 'CLI Reference', link: '/docs/cli/' },
          { label: 'deku.toml', link: '/docs/deku-toml/' },
          { label: 'Plugin API', link: '/docs/plugins/' },
        ]},
      ],
    }),
  ],
});
