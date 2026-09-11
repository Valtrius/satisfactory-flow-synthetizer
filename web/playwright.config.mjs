import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  testDir: './tests',
  testMatch: '*.spec.mjs',
  workers: 1,
  timeout: 120_000,
  outputDir: '../target/web-backends/test-results',
  reporter: [
    ['list'],
    ['json', { outputFile: fileURLToPath(new URL('../target/web-backends/browser-results.json', import.meta.url)) }],
  ],
  use: {
    browserName: 'chromium',
    channel: process.env.WEB_TEST_CHANNEL || undefined,
    baseURL: 'http://127.0.0.1:4178/satisfactory-flow-synthetizer/',
    headless: true,
  },
  webServer: {
    command: 'node web/tests/server.mjs',
    cwd: fileURLToPath(new URL('..', import.meta.url)),
    url: 'http://127.0.0.1:4178/satisfactory-flow-synthetizer/',
    reuseExistingServer: false,
  },
});
