import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  testDir: '.',
  testMatch: '*.spec.mjs',
  workers: 1,
  timeout: 60_000,
  outputDir: '../../target/web-platform/test-results',
  reporter: [
    ['list'],
    ['json', { outputFile: fileURLToPath(new URL('../../target/web-platform/results.json', import.meta.url)) }],
  ],
  use: {
    browserName: 'chromium',
    channel: process.env.WEB_TEST_CHANNEL || undefined,
    baseURL: 'http://127.0.0.1:4179/satisfactory-flow-synthetizer/',
    headless: true,
  },
  webServer: {
    command: 'npm run dev -w frontend -- --host 127.0.0.1 --port 4179 --base /satisfactory-flow-synthetizer/',
    cwd: fileURLToPath(new URL('../..', import.meta.url)),
    url: 'http://127.0.0.1:4179/satisfactory-flow-synthetizer/',
    reuseExistingServer: false,
  },
});
