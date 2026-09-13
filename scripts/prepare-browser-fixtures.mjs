import { execFileSync } from 'node:child_process';

// Native cvc5 is needed only to prepare the independent comparison corpus.
// CI may reuse the Windows-produced corpus and build just the verifier example.
execFileSync(
  'cargo',
  ['build', '-p', 'solver-portable-tests', '--example', 'verify_browser_jobs', '--locked', '-j', '2'],
  { stdio: 'inherit', windowsHide: true },
);
