import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import type { Plugin } from 'vite';

const names = ['solver_web.js', 'solver_web_bg.wasm'];

/** Ship the matching verifier pair locally, with content-versioned URLs. No cvc5 is copied. */
export function verifierAssets(): Plugin {
  const files = names.map((name) => fileURLToPath(new URL(`../target/web-backends/verifier/${name}`, import.meta.url)));
  const available = files.every((path) => existsSync(path));
  const contents = available ? files.map((path) => readFileSync(path)) : [];
  const digest = createHash('sha256');
  for (const content of contents) digest.update(content);
  const directory = `assets/verifier-${digest.digest('hex').slice(0, 16)}`;
  let required = false;
  return {
    name: 'local-solver-verifier',
    config: () => ({ define: { __SFS_VERIFIER_DIR__: JSON.stringify(directory) } }),
    configResolved(config) {
      required = config.command === 'build';
    },
    buildStart() {
      if (required && !available) this.error('Build the verifier before the frontend: npm run build:web:verifier');
    },
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const name = names.find((name) => req.url?.split('?')[0]?.endsWith(`/${directory}/${name}`));
        if (!name) {
          next();
          return;
        }
        if (!available) {
          res.statusCode = 503;
          res.end('Build the verifier with npm run build:web:verifier');
          return;
        }
        res.setHeader('Content-Type', name.endsWith('.wasm') ? 'application/wasm' : 'text/javascript');
        res.setHeader('Cache-Control', 'no-cache');
        res.end(contents[names.indexOf(name)]);
      });
    },
    generateBundle() {
      for (const [index, name] of names.entries())
        this.emitFile({ type: 'asset', fileName: `${directory}/${name}`, source: contents[index] });
    },
  };
}
