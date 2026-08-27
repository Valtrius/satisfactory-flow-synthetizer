import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  base: './',
  plugins: [svelte(), tailwindcss()],
  clearScreen: false,
  build: {
    // ELK is intentionally lazy-loaded only when a graph is laid out. Its standalone bundled
    // worker is ~1.4 MB; keep the warning threshold tight enough to catch growth elsewhere.
    chunkSizeWarningLimit: 1500
  },
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**']
    }
  },
  test: {
    environment: 'jsdom'
  }
});
