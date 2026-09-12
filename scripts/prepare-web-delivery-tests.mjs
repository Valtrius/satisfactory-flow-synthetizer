import { cp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';

const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const target = resolve('target/web-delivery');
await mkdir(target, { recursive: true });
for (const variant of ['a', 'b']) await cp(resolve('frontend/dist'), resolve(target, variant), { recursive: true });
const release = JSON.parse(await readFile(resolve(target, 'b/release.json'), 'utf8'));
const next = digest(release.buildId + ':test-update').slice(0, 24);
const html = (await readFile(resolve(target, 'b/index.html'), 'utf8')).replace(release.buildId, next);
release.buildId = next;
const index = release.assets.find((asset) => asset.path === 'index.html');
index.sha256 = digest(html);
index.bytes = Buffer.byteLength(html);
await writeFile(resolve(target, 'b/index.html'), html);
await writeFile(resolve(target, 'b/release.json'), JSON.stringify(release));
const worker = await readFile('frontend/offline/service-worker.js', 'utf8');
await writeFile(
  resolve(target, 'b/service-worker.js'),
  worker.replace("'__SFS_RELEASE_MANIFEST__'", JSON.stringify(release)),
);
console.log('Prepared two production static builds for offline lifecycle tests.');
