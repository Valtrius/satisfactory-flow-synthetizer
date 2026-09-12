import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Plugin } from 'vite';

const root = fileURLToPath(new URL('..', import.meta.url));
const digest = (content: Uint8Array) => createHash('sha256').update(content).digest('hex');

/** One content-versioned local search distribution. Desktop builds omit it. */
export function solverAssets(desktop: boolean): Plugin {
  const sources = new Map([
    ['solver_browser.js', 'target/web-backends/solver/solver_browser.js'],
    ['solver_browser_bg.wasm', 'target/web-backends/solver/solver_browser_bg.wasm'],
    ['cvc5.mjs', 'target/web-backends/cvc5/cvc5.mjs'],
    ['cvc5.wasm', 'target/web-backends/cvc5/cvc5.wasm'],
    ['session.mjs', 'web/cvc5/session.mjs'],
    ['cvc5-build.json', 'target/web-backends/cvc5/build.json'],
    ['licenses/canonaut/LICENSE', 'vendor/canonaut/LICENSE'],
    ['licenses/canonaut/NOTICE', 'vendor/canonaut/NOTICE'],
    ['licenses/canonaut/PORTABILITY.md', 'vendor/canonaut/PORTABILITY.md'],
    ['licenses/application/LICENSE', 'LICENSE'],
  ]);
  const addNotices = (relative: string, prefix: string) => {
    for (const entry of readdirSync(join(root, relative), { withFileTypes: true })) {
      if (entry.isDirectory()) addNotices(`${relative}/${entry.name}`, `${prefix}/${entry.name}`);
      else if (entry.isFile()) sources.set(`${prefix}/${entry.name}`, `${relative}/${entry.name}`);
    }
  };
  if (!desktop && existsSync(join(root, 'target/web-backends/cvc5/licenses')))
    addNotices('target/web-backends/cvc5/licenses', 'licenses');
  const available = !desktop && [...sources.values()].every((path) => existsSync(join(root, path)));
  const contents = new Map<string, Buffer>();
  let invalid = '';
  if (available) {
    for (const [name, path] of sources) contents.set(name, readFileSync(join(root, path)));
    try {
      const build = JSON.parse(contents.get('cvc5-build.json')!.toString());
      for (const name of ['cvc5.mjs', 'cvc5.wasm']) {
        const content = contents.get(name)!;
        if (build.artifacts[name].bytes !== content.length || build.artifacts[name].sha256 !== digest(content))
          throw new Error(`Mismatched cvc5 artifact: ${name}`);
      }
    } catch (error) {
      invalid = String(error);
    }
    const manifest = {
      schema: 1,
      protocol: 1,
      assets: Object.fromEntries(
        [...contents].map(([name, content]) => [name, { bytes: content.length, sha256: digest(content) }]),
      ),
    };
    contents.set('manifest.json', Buffer.from(JSON.stringify(manifest, null, 2)));
  }
  const combined = createHash('sha256');
  for (const [name, content] of contents) {
    combined.update(name);
    combined.update(content);
  }
  const directory = `assets/solver-${combined.digest('hex').slice(0, 16)}`;
  let required = false;
  return {
    name: 'local-browser-solver',
    config: () => ({ define: { __SFS_SOLVER_DIR__: JSON.stringify(directory) } }),
    configResolved(config) {
      required = !desktop && config.command === 'build';
    },
    buildStart() {
      if (required && (!available || invalid))
        this.error(invalid || 'Build browser solver assets first: npm run build:web:assets');
    },
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const url = req.url?.split('?')[0] ?? '';
        const marker = `/${directory}/`;
        const index = url.indexOf(marker);
        if (desktop || index < 0) {
          next();
          return;
        }
        const name = url.slice(index + marker.length);
        if (!available || invalid) {
          res.statusCode = 503;
          res.end(invalid || 'Build browser solver assets: npm run build:web:assets');
          return;
        }
        const content = contents.get(name);
        if (!content) {
          res.statusCode = 404;
          res.end();
          return;
        }
        res.setHeader(
          'Content-Type',
          name.endsWith('.wasm')
            ? 'application/wasm'
            : /\.m?js$/.test(name)
              ? 'text/javascript'
              : name.endsWith('.json')
                ? 'application/json'
                : 'text/plain',
        );
        res.setHeader('Cache-Control', 'no-cache');
        res.end(content);
      });
    },
    generateBundle() {
      if (!desktop)
        for (const [name, source] of contents)
          this.emitFile({ type: 'asset', fileName: `${directory}/${name}`, source });
    },
  };
}
