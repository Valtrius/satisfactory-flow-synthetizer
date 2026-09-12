import type { Plugin } from 'vite';
import { localAssets } from './localAssets.ts';

/** Ship only the production verifier pair, independently of browser search. */
export function verifierAssets(): Plugin {
  return localAssets({
    name: 'local-solver-verifier',
    namespace: 'verifier',
    define: '__SFS_VERIFIER_DIR__',
    sources: new Map(
      ['solver_web.js', 'solver_web_bg.wasm'].map((name) => [name, `target/web-backends/verifier/${name}`]),
    ),
    missing: 'Build the verifier before the frontend: npm run build:web:verifier',
  });
}
