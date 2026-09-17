import { defineConfig } from 'astro/config';
import react from '@astrojs/react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  integrations: [react()],
  vite: {
    plugins: [tailwindcss()],
  },
  output: 'static',
  outDir: './dist',
  // Astro 7 defaults to JSX whitespace stripping; keep the Astro 6 HTML-aware output.
  compressHTML: true,
  build: {
    assets: '_assets',
  },
});
