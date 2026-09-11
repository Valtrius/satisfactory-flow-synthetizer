import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, extname, sep } from 'node:path';

const root = resolve('frontend/dist');
const prefix = '/satisfactory-flow-synthetizer/';
createServer(async (request, response) => {
  try {
    const path = new URL(request.url, 'http://localhost').pathname;
    if (!path.startsWith(prefix)) {
      response.writeHead(404).end();
      return;
    }
    const file = resolve(root, path.slice(prefix.length) || 'index.html');
    if (!file.startsWith(root + sep)) {
      response.writeHead(403).end();
      return;
    }
    const content = await readFile(file);
    response.setHeader(
      'Content-Type',
      {
        '.html': 'text/html',
        '.js': 'text/javascript',
        '.css': 'text/css',
        '.wasm': 'application/wasm',
        '.svg': 'image/svg+xml',
      }[extname(file)] || 'application/octet-stream',
    );
    response.setHeader(
      'Content-Security-Policy',
      "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' http://127.0.0.1:4180; object-src 'none'; base-uri 'none'",
    );
    response.setHeader('Referrer-Policy', 'no-referrer');
    response.end(content);
  } catch {
    response.writeHead(404).end();
  }
}).listen(4182, '127.0.0.1');
