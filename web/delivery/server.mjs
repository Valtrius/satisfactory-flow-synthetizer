import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, resolve, sep } from 'node:path';

let variant = 'a';
let failure = null;
let held;
const scopes = ['/satisfactory-flow-synthetizer/', '/second-app/'];
createServer(async (request, response) => {
  try {
    const url = new URL(request.url, 'http://localhost');
    if (url.pathname === '/__control' && request.method === 'POST') {
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      const value = JSON.parse(Buffer.concat(chunks).toString());
      if (!['a', 'b'].includes(value.variant)) throw new Error('Unknown fixture');
      variant = value.variant;
      failure = value.failure ?? null;
      if (Object.hasOwn(value, 'hold')) {
        held?.release();
        held = value.hold ? { path: value.hold, reached: false } : undefined;
        if (held)
          held.promise = new Promise((resolve) => {
            held.release = resolve;
          });
      }
      response.end('ok');
      return;
    }
    if (url.pathname === '/__held') {
      response.setHeader('Content-Type', 'application/json');
      response.end(JSON.stringify(Boolean(held?.reached)));
      return;
    }
    if (url.pathname === '/__outside') {
      response.setHeader('Content-Type', 'text/html');
      response.end('<!doctype html><title>Outside app scope</title>');
      return;
    }
    const scope = scopes.find((prefix) => url.pathname.startsWith(prefix));
    if (!scope) {
      response.writeHead(404).end();
      return;
    }
    const relative = decodeURIComponent(url.pathname.slice(scope.length)) || 'index.html';
    if (failure && relative.endsWith(failure.path)) {
      response
        .writeHead(failure.kind === 'corrupt' ? 200 : 503, { 'Content-Type': 'text/html' })
        .end('<!doctype html>Injected asset failure');
      return;
    }
    const root = resolve('target/web-delivery', variant);
    const path = resolve(root, relative);
    if (!path.startsWith(root + sep)) {
      response.writeHead(403).end();
      return;
    }
    const content = await readFile(path);
    if (held && relative === held.path) {
      held.reached = true;
      await held.promise;
    }
    response.setHeader(
      'Content-Type',
      {
        '.html': 'text/html',
        '.js': 'text/javascript',
        '.mjs': 'text/javascript',
        '.css': 'text/css',
        '.json': 'application/json',
        '.wasm': 'application/wasm',
        '.svg': 'image/svg+xml',
      }[extname(path)] ?? 'application/octet-stream',
    );
    response.setHeader('Cache-Control', 'no-store');
    // No custom CSP/isolation headers: qualify the HTML policy used by Pages.
    response.end(content);
  } catch {
    response.writeHead(404).end();
  }
}).listen(4185, '127.0.0.1');
