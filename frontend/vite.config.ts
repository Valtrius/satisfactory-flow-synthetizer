import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';
import { verifierAssets } from './verifierAssets.ts';
import { solverAssets } from './solverAssets.ts';

export default defineConfig(({ mode }) => ({
  base: './',
  plugins: [svelte(), tailwindcss(), verifierAssets(), solverAssets(mode === 'desktop')],
  clearScreen: false,
  build: {
    // ELK is intentionally lazy-loaded only when a graph is laid out. Its standalone bundled
    // worker is ~1.4 MB; keep the warning threshold tight enough to catch growth elsewhere.
    chunkSizeWarningLimit: 1500,
  },
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  test: {
    projects: [
      {
        extends: true,
        test: { name: 'unit', environment: 'jsdom', include: ['src/**/*.test.ts'], exclude: ['src/**/*.dom.test.ts'] },
      },
      {
        extends: true,
        resolve: { conditions: ['browser'] },
        test: { name: 'dom', environment: 'jsdom', include: ['src/**/*.dom.test.ts'] },
      },
    ],
  },
}));
