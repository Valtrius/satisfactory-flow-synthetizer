import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';
import { solverAssets } from '../../frontend/solverAssets.ts';

export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
  base: '/satisfactory-flow-synthetizer/',
  plugins: [solverAssets(false)],
  build: { outDir: '../../target/platform-benchmark-web', emptyOutDir: true },
});
