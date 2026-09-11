import fs from 'node:fs';
import { execFileSync } from 'node:child_process';
import { verificationFixtures } from '../web/tests/fixtures.mjs';

function native(fixtures) {
  return JSON.parse(
    execFileSync(
      'cargo',
      ['run', '-p', 'solver-web', '--example', 'verify_fixtures', '--locked', '--quiet', '-j', '2'],
      {
        input: JSON.stringify(fixtures),
        encoding: 'utf8',
        maxBuffer: 16 * 1024 * 1024,
        windowsHide: true,
      },
    ),
  );
}
const physical = native(verificationFixtures()).filter((fixture) => fixture.native.kind === 'verified');
const migrations = physical.map((fixture) => {
  const request = JSON.parse(fixture.payload).request;
  const endpoints = (values) => values.map(({ name, rate }) => ({ name, rate }));
  return {
    name: fixture.name,
    operation: 'presentation-share',
    expectedKind: 'verified-share',
    payload: JSON.stringify({
      request: { inputs: endpoints(request.inputs), outputs: endpoints(request.outputs), beltRate: request.beltRate },
      solution: fixture.native.solution,
    }),
  };
});
const migrated = native(migrations);
const verified = native(
  migrated.map((fixture) => ({
    name: fixture.name,
    operation: 'share',
    expectedKind: 'verified-share',
    payload: JSON.stringify(fixture.native.share),
  })),
);
const result = migrated.map((fixture, index) => ({ ...fixture, opened: verified[index].native }));
fs.mkdirSync('target/web-shares', { recursive: true });
fs.writeFileSync('target/web-shares/fixtures.json', JSON.stringify(result));
console.log(`Prepared ${result.length} native selected-solution round trips.`);
