import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  testDir: '.',
  testMatch: '*.spec.mjs',
  workers: 1,
  timeout: 120_000,
  outputDir: '../../target/web-delivery/test-results',
  reporter: [
    ['list'],
    ['json', { outputFile: fileURLToPath(new URL('../../target/web-delivery/results.json', import.meta.url)) }],
  ],
  use: {
    browserName: 'chromium',
    channel: process.env.WEB_TEST_CHANNEL || undefined,
    baseURL: 'http://127.0.0.1:4185/satisfactory-flow-synthetizer/',
    headless: true,
  },
  webServer: {
    command: 'node web/delivery/server.mjs',
    cwd: fileURLToPath(new URL('../..', import.meta.url)),
    url: 'http://127.0.0.1:4185/__outside',
    reuseExistingServer: false,
  },
});
