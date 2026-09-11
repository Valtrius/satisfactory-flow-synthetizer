import fs from 'node:fs';
import { execFileSync } from 'node:child_process';
import { verificationFixtures } from '../web/tests/fixtures.mjs';

const results = execFileSync(
  'cargo',
  ['run', '-p', 'solver-web', '--example', 'verify_fixtures', '--locked', '--quiet', '-j', '2'],
  {
    input: JSON.stringify(verificationFixtures()),
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
    windowsHide: true,
  },
);
fs.mkdirSync('target/web-backends', { recursive: true });
fs.writeFileSync('target/web-backends/fixtures.json', results);
console.log(`Prepared ${JSON.parse(results).length} native verification fixtures.`);
