import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import type { Plugin } from 'vite';

export const WEB_CSP =
  "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'";
const hash = (value: Uint8Array | string) => createHash('sha256').update(value).digest('hex');
const loadProgressMarkup = `
    <style>
      #sfs-web-load-progress {
        position: fixed;
        z-index: 2147483647;
        top: 0;
        right: 0;
        left: 0;
        height: 3px;
        overflow: hidden;
        pointer-events: none;
      }
      #sfs-web-load-progress[hidden] { display: none; }
      #sfs-web-load-progress-bar {
        height: 100%;
        background: linear-gradient(90deg, #ffab6b, #ff8a3d);
        box-shadow: 0 0 10px rgb(255 128 52 / 45%);
      }
      #sfs-web-load-progress[data-mode='determinate'] #sfs-web-load-progress-bar {
        width: var(--sfs-load-progress, 0%);
        transition: width 100ms linear;
      }
      #sfs-web-load-progress[data-mode='indeterminate'] #sfs-web-load-progress-bar {
        width: 32%;
        animation: sfs-web-load-progress 1.1s ease-in-out infinite;
      }
      @keyframes sfs-web-load-progress {
        0% { transform: translateX(-120%); }
        50% { transform: translateX(150%); }
        100% { transform: translateX(330%); }
      }
      @media (prefers-reduced-motion: reduce) {
        #sfs-web-load-progress[data-mode='indeterminate'] #sfs-web-load-progress-bar {
          width: 100%;
          animation: none;
          opacity: 0.6;
        }
      }
    </style>`;
const loadProgressElement = `
    <div id="sfs-web-load-progress" data-mode="indeterminate" role="progressbar" aria-label="Loading application" aria-valuemin="0" aria-valuemax="100">
      <div id="sfs-web-load-progress-bar"></div>
    </div>`;

/** Bind the HTML, lazy modules and offline worker to the same emitted build. */
export function webDelivery(desktop: boolean, release: boolean): Plugin {
  return {
    name: 'versioned-browser-delivery',
    apply: 'build',
    enforce: 'post',
    generateBundle: {
      order: 'post',
      handler(_options, bundle) {
        if (desktop) return;
        const distribution = new URL('../target/web-backends/distribution/', import.meta.url);
        const recordUrl = new URL('source-manifest.json', distribution);
        const downloads: { path: string; bytes: number; sha256: string }[] = [];
        const extra = new Map<string, Buffer>();
        const emit = (path: string, source: Uint8Array | string) => {
          const bytes = Buffer.from(source);
          extra.set(path, bytes);
          this.emitFile({ type: 'asset', fileName: path, source: bytes });
        };
        if (release) {
          if (!existsSync(recordUrl)) this.error('Prepare source/relink materials: npm run build:web:release');
          const recordBytes = readFileSync(recordUrl);
          const record = JSON.parse(recordBytes.toString());
          for (const [path, expected] of Object.entries(record.sourceFiles) as [string, string][]) {
            if (hash(readFileSync(new URL(`../${path}`, import.meta.url))) !== expected)
              this.error(`Source snapshot is stale: ${path}`);
          }
          for (const [path, expected] of Object.entries(record.files) as [
            string,
            { bytes: number; sha256: string },
          ][]) {
            if (!/^(sources\/[a-zA-Z0-9.-]+|licenses\.html)$/.test(path))
              this.error(`Invalid distribution path: ${path}`);
            const source = readFileSync(new URL(path, distribution));
            if (hash(source) !== expected.sha256 || source.length !== expected.bytes)
              this.error(`Invalid distribution file: ${path}`);
            emit(path, source);
            if (path.startsWith('sources/')) downloads.push({ path, ...expected });
          }
          emit('source-manifest.json', recordBytes);
        } else {
          emit(
            'licenses.html',
            '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Development build</title><main><h1>Development build</h1><p>This build does not include the source distribution. Do not publish it. Use npm run build:web:release to include third-party notices and source/relink downloads.</p><a href="./">Return to the app</a></main></html>',
          );
        }
        const index = bundle['index.html'];
        if (!index || index.type !== 'asset') this.error('Browser delivery requires index.html.');
        const worker = readFileSync(new URL('./offline/service-worker.js', import.meta.url), 'utf8');
        const contents = () => {
          const files = new Map<string, Uint8Array>();
          for (const [path, file] of Object.entries(bundle))
            files.set(path, Buffer.from(file.type === 'chunk' ? file.code : file.source));
          for (const [path, bytes] of extra) files.set(path, bytes);
          return [...files]
            .sort(([left], [right]) => left.localeCompare(right))
            .map(([path, bytes]) => ({ path, bytes }));
        };
        const identity = createHash('sha256').update(worker).update(WEB_CSP);
        for (const file of contents()) identity.update(file.path).update(file.bytes);
        const buildId = identity.digest('hex').slice(0, 24);
        index.source = String(index.source).replace(
          '<head>',
          `<head>\n    <meta http-equiv="Content-Security-Policy" content="${WEB_CSP}" />\n    <meta name="referrer" content="no-referrer" />\n    <meta name="sfs-build" content="${buildId}" />${loadProgressMarkup}`,
        );
        index.source = String(index.source).replace('<body>', `<body>${loadProgressElement}`);
        const manifest = {
          schema: 1,
          buildId,
          distribution: release,
          downloads,
          assets: contents()
            .filter(({ path }) => !path.startsWith('sources/'))
            .map(({ path, bytes }) => ({
              path,
              bytes: bytes.length,
              sha256: hash(bytes),
              group: path.startsWith('assets/solver-')
                ? 'solver'
                : path.startsWith('assets/verifier-')
                  ? 'verifier'
                  : 'shell',
            })),
        };
        this.emitFile({ type: 'asset', fileName: 'release.json', source: JSON.stringify(manifest, null, 2) + '\n' });
        this.emitFile({
          type: 'asset',
          fileName: 'service-worker.js',
          source: worker.replace("'__SFS_RELEASE_MANIFEST__'", JSON.stringify(manifest)),
        });
        this.emitFile({ type: 'asset', fileName: '.nojekyll', source: '' });
      },
    },
  };
}
