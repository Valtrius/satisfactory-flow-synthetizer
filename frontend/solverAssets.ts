import { existsSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import type { Plugin } from 'vite';
import { localAssets, assetRoot, assetDigest } from './localAssets.ts';

/** Desktop bundles omit search assets; cvc5 artifacts must match their build provenance. */
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
    for (const entry of readdirSync(join(assetRoot, relative), { withFileTypes: true })) {
      if (entry.isDirectory()) addNotices(`${relative}/${entry.name}`, `${prefix}/${entry.name}`);
      else if (entry.isFile()) sources.set(`${prefix}/${entry.name}`, `${relative}/${entry.name}`);
    }
  };
  if (!desktop && existsSync(join(assetRoot, 'target/web-backends/cvc5/licenses')))
    addNotices('target/web-backends/cvc5/licenses', 'licenses');
  return localAssets({
    name: 'local-browser-solver',
    namespace: 'solver',
    define: '__SFS_SOLVER_DIR__',
    enabled: !desktop,
    sources,
    missing: 'Build browser solver assets first: npm run build:web:assets',
    prepare(contents) {
      const build = JSON.parse(contents.get('cvc5-build.json')!.toString());
      for (const name of ['cvc5.mjs', 'cvc5.wasm']) {
        const content = contents.get(name)!;
        if (build.artifacts[name].bytes !== content.length || build.artifacts[name].sha256 !== assetDigest(content))
          throw new Error(`Mismatched cvc5 artifact: ${name}`);
      }
      const manifest = {
        schema: 1,
        protocol: 2,
        assets: Object.fromEntries(
          [...contents].map(([name, content]) => [name, { bytes: content.length, sha256: assetDigest(content) }]),
        ),
      };
      contents.set('manifest.json', Buffer.from(JSON.stringify(manifest, null, 2)));
    },
  });
}
