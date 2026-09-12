import { cp, mkdir, readFile, writeFile, rename, rm } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve, sep } from 'node:path';

const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const target = resolve('target/web-delivery');
await mkdir(target, { recursive: true });
for (const variant of ['a', 'b']) {
  const directory = resolve(target, variant);
  if (!directory.startsWith(target + sep)) throw new Error('Fixture directory escaped target/web-delivery.');
  await rm(directory, { recursive: true, force: true });
  await cp(resolve('frontend/dist'), directory, { recursive: true });
}
const release = JSON.parse(await readFile(resolve(target, 'b/release.json'), 'utf8'));
const next = digest(release.buildId + ':test-update').slice(0, 24);
// Model a deployment that replaces content-versioned runtime directories.
// Changing only the build ID leaves the missing-old-asset regression invisible.
const replacements = new Map([[release.buildId, next]]);
for (const asset of release.assets) {
  const directory = asset.path.match(/^assets\/((?:solver|verifier)-[^/]+)\//)?.[1];
  if (directory) replacements.set(directory, `${directory.split('-')[0]}-${digest(directory + next).slice(0, 16)}`);
}
for (const [before, after] of [...replacements].slice(1)) {
  const from = resolve(target, 'b/assets', before);
  const to = resolve(target, 'b/assets', after);
  if (![from, to].every((path) => path.startsWith(resolve(target, 'b/assets') + sep)))
    throw new Error('Runtime fixture directory escaped b/assets.');
  await rename(from, to);
}
const replace = (text) => {
  for (const [before, after] of replacements) text = text.replaceAll(before, after);
  return text;
};
for (const asset of release.assets) {
  asset.path = replace(asset.path);
  const path = resolve(target, 'b', asset.path);
  if (!path.startsWith(resolve(target, 'b') + sep)) throw new Error('Fixture asset escaped b/.');
  let bytes = await readFile(path);
  if (/\.(?:m?js|html|json)$/.test(asset.path)) {
    bytes = Buffer.from(replace(bytes.toString('utf8')));
    await writeFile(path, bytes);
  }
  asset.sha256 = digest(bytes);
  asset.bytes = bytes.length;
}
release.buildId = next;
await writeFile(resolve(target, 'b/release.json'), JSON.stringify(release));
const worker = await readFile('frontend/offline/service-worker.js', 'utf8');
await writeFile(
  resolve(target, 'b/service-worker.js'),
  worker.replace("'__SFS_RELEASE_MANIFEST__'", JSON.stringify(release)),
);
console.log('Prepared two production static builds for offline lifecycle tests.');
