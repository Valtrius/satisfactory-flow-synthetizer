import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../..', import.meta.url));
export default defineConfig({
  testDir: '.',
  testMatch: '*.spec.mjs',
  workers: 1,
  timeout: 120_000,
  outputDir: '../../target/web-solver/test-results',
  reporter: [
    ['list'],
    ['json', { outputFile: fileURLToPath(new URL('../../target/web-solver/results.json', import.meta.url)) }],
  ],
  use: {
    browserName: 'chromium',
    channel: process.env.WEB_TEST_CHANNEL || undefined,
    baseURL: 'http://127.0.0.1:4184/satisfactory-flow-synthetizer/',
    headless: true,
  },
  webServer: [
    {
      command: 'npm run dev -w frontend -- --host 127.0.0.1 --port 4184 --base /satisfactory-flow-synthetizer/',
      cwd: root,
      url: 'http://127.0.0.1:4184/satisfactory-flow-synthetizer/',
      reuseExistingServer: false,
    },
    {
      command: 'node web/shares/static-server.mjs',
      cwd: root,
      url: 'http://127.0.0.1:4182/satisfactory-flow-synthetizer/',
      reuseExistingServer: false,
    },
  ],
});
