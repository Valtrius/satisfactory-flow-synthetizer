import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Plugin } from 'vite';

export const assetRoot = fileURLToPath(new URL('..', import.meta.url));
export const assetDigest = (content: Uint8Array) => createHash('sha256').update(content).digest('hex');

type LocalAssetsOptions = {
  name: string;
  namespace: string;
  define: string;
  sources: Map<string, string>;
  missing: string;
  enabled?: boolean;
  prepare?: (contents: Map<string, Buffer>) => void;
};

/** Serve and emit one validated local distribution with content-versioned URLs. */
export function localAssets(options: LocalAssetsOptions): Plugin {
  const enabled = options.enabled !== false;
  const available = enabled && [...options.sources.values()].every((path) => existsSync(join(assetRoot, path)));
  const contents = new Map<string, Buffer>();
  let invalid = '';
  if (available) {
    for (const [name, path] of options.sources) contents.set(name, readFileSync(join(assetRoot, path)));
    try {
      options.prepare?.(contents);
    } catch (error) {
      invalid = String(error);
    }
  }
  const combined = createHash('sha256');
  for (const [name, content] of contents) combined.update(name).update(content);
  const directory = `assets/${options.namespace}-${combined.digest('hex').slice(0, 16)}`;
  let required = false;
  return {
    name: options.name,
    config: () => ({ define: { [options.define]: JSON.stringify(directory) } }),
    configResolved(config) {
      required = enabled && config.command === 'build';
    },
    buildStart() {
      if (required && (!available || invalid)) this.error(invalid || options.missing);
    },
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const url = req.url?.split('?')[0] ?? '';
        const marker = `/${directory}/`;
        const index = url.indexOf(marker);
        if (!enabled || index < 0) {
          next();
          return;
        }
        if (!available || invalid) {
          res.statusCode = 503;
          res.end(invalid || options.missing);
          return;
        }
        const name = url.slice(index + marker.length);
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
      if (enabled)
        for (const [name, source] of contents)
          this.emitFile({ type: 'asset', fileName: `${directory}/${name}`, source });
    },
  };
}
