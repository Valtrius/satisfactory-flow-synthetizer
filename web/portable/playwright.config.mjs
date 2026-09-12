import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  testDir: '.',
  testMatch: '*.spec.mjs',
  workers: 1,
  timeout: 180_000,
  outputDir: '../../target/web-portable/test-results',
  reporter: [
    ['list'],
    ['json', { outputFile: fileURLToPath(new URL('../../target/web-portable/results.json', import.meta.url)) }],
  ],
  use: {
    browserName: 'chromium',
    channel: process.env.WEB_TEST_CHANNEL || undefined,
    headless: true,
    baseURL: 'http://127.0.0.1:4183/satisfactory-flow-synthetizer/',
  },
  webServer: {
    command: 'node web/portable/server.mjs',
    cwd: fileURLToPath(new URL('../..', import.meta.url)),
    url: 'http://127.0.0.1:4183/satisfactory-flow-synthetizer/',
    reuseExistingServer: false,
  },
});
