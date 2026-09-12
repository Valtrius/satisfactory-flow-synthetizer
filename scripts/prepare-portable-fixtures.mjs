import fs from 'node:fs';
import { execFileSync } from 'node:child_process';

const results = execFileSync(
  'cargo',
  ['run', '-p', 'solver-portable-tests', '--example', 'prepare_search', '--locked', '--quiet', '-j', '2'],
  { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, windowsHide: true },
);
const cases = JSON.parse(results);
fs.mkdirSync('target/web-portable', { recursive: true });
fs.writeFileSync('target/web-portable/solves.json', results);
console.log(`Prepared ${cases.length} native/reference exact-search comparisons.`);
