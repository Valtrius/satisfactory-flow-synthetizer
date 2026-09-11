import http from 'node:http';
import fs from 'node:fs';

const base = '/satisfactory-flow-synthetizer/';
const files = new Map([
  ['', ['web/tests/index.html', 'text/html; charset=utf-8']],
  ['worker.mjs', ['web/tests/worker.mjs', 'text/javascript']],
  ['session.mjs', ['web/cvc5/session.mjs', 'text/javascript']],
  ['fixtures.json', ['target/web-backends/fixtures.json', 'application/json']],
  ['solver_web.js', ['target/web-backends/verifier/solver_web.js', 'text/javascript']],
  ['solver_web_bg.wasm', ['target/web-backends/verifier/solver_web_bg.wasm', 'application/wasm']],
  ['cvc5.mjs', ['target/web-backends/cvc5/cvc5.mjs', 'text/javascript']],
  ['cvc5.wasm', ['target/web-backends/cvc5/cvc5.wasm', 'application/wasm']],
]);
const server = http.createServer((request, response) => {
  const pathname = new URL(request.url, 'http://localhost').pathname;
  const entry = pathname.startsWith(base) ? files.get(pathname.slice(base.length)) : undefined;
  if (!entry || !fs.existsSync(entry[0])) {
    response.writeHead(404).end('Not found');
    return;
  }
  response.writeHead(200, { 'Content-Type': entry[1], 'Cache-Control': 'no-store' });
  fs.createReadStream(entry[0]).pipe(response);
});
server.listen(4178, '127.0.0.1', () => console.log(`Backend test server: http://127.0.0.1:4178${base}`));
