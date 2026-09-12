import http from 'node:http';
import fs from 'node:fs';

const base = '/satisfactory-flow-synthetizer/';
const files = new Map([
  ['', ['web/portable/index.html', 'text/html; charset=utf-8']],
  ['worker.mjs', ['web/portable/worker.mjs', 'text/javascript']],
  ['identity.json', ['crates/solver-portable-tests/fixtures/identity-v1.json', 'application/json']],
  ['solves.json', ['target/web-portable/solves.json', 'application/json']],
  ['solver_portable_tests.js', ['target/web-backends/portable/solver_portable_tests.js', 'text/javascript']],
  ['solver_portable_tests_bg.wasm', ['target/web-backends/portable/solver_portable_tests_bg.wasm', 'application/wasm']],
  ['session.mjs', ['web/cvc5/session.mjs', 'text/javascript']],
  ['cvc5.mjs', ['target/web-backends/cvc5/cvc5.mjs', 'text/javascript']],
  ['cvc5.wasm', ['target/web-backends/cvc5/cvc5.wasm', 'application/wasm']],
]);
http
  .createServer((request, response) => {
    const pathname = new URL(request.url, 'http://localhost').pathname;
    const entry = pathname.startsWith(base) ? files.get(pathname.slice(base.length)) : undefined;
    if (!entry || !fs.existsSync(entry[0])) {
      response.writeHead(404).end('Not found');
      return;
    }
    response.writeHead(200, { 'Content-Type': entry[1], 'Cache-Control': 'no-store' });
    fs.createReadStream(entry[0]).pipe(response);
  })
  .listen(4183, '127.0.0.1');
