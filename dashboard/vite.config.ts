import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// In development the dashboard talks to a local dekud. Proxying keeps every
// request same-origin, which the session cookie and CSRF check require; the
// Origin header is rewritten to the daemon's so its origin check passes.
const daemon = process.env.DEKU_DAEMON_URL ?? 'http://127.0.0.1:2810';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 4330,
    proxy: {
      '/api': { target: daemon, changeOrigin: true, headers: { origin: daemon } },
      '/healthz': { target: daemon, changeOrigin: true },
    },
  },
});
