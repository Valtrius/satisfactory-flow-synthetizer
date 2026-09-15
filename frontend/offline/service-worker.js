/* This file is emitted with a build-specific manifest. It never executes solver work. */
const release = '__SFS_RELEASE_MANIFEST__';
const scope = self.registration.scope;
const prefix = `sfs-offline-v1:${scope}:`;
const cacheName = prefix + release.buildId;
const assets = new Map(release.assets.map((asset) => [new URL(asset.path, scope).href, asset]));
let preparation;
const progressStep = 512 * 1024;

async function broadcastProgress(loaded, total, state = 'progress') {
  const clients = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
  for (const client of clients)
    if (client.url.startsWith(scope)) client.postMessage({ kind: 'offline-progress', loaded, total, state });
}

function progressReporter(total) {
  let last = -progressStep;
  return async (loaded, state = 'progress', force = false) => {
    const bounded = Math.max(0, Math.min(total, loaded));
    if (!force && state === 'progress' && bounded - last < progressStep) return;
    last = bounded;
    await broadcastProgress(bounded, total, state);
  };
}

async function cachedStatus() {
  const cache = await caches.open(cacheName);
  let cached = 0;
  let bytes = 0;
  let shellComplete = true;
  for (const [url, asset] of assets) {
    if (await cache.match(url)) {
      cached++;
      bytes += asset.bytes;
    } else if (asset.group === 'shell') shellComplete = false;
  }
  const complete = cached === assets.size;
  return { buildId: release.buildId, cached, total: assets.size, bytes, complete, shellComplete };
}

async function readVerifiedResponse(response, asset, onProgress) {
  const chunks = [];
  let length = 0;
  if (response.body) {
    const reader = response.body.getReader();
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      chunks.push(value);
      length += value.byteLength;
      await onProgress?.(length);
    }
  } else {
    const value = new Uint8Array(await response.arrayBuffer());
    chunks.push(value);
    length = value.byteLength;
    await onProgress?.(length);
  }
  const body = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    body.set(chunk, offset);
    offset += chunk.byteLength;
  }
  const digest = [...new Uint8Array(await crypto.subtle.digest('SHA-256', body))]
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
  if (body.byteLength !== asset.bytes || digest !== asset.sha256)
    throw new Error(`Version check failed for ${asset.path}. The previous offline build was kept.`);
  return new Response(body, { status: response.status, statusText: response.statusText, headers: response.headers });
}

async function download(url, asset, required, onProgress) {
  const cache = await caches.open(cacheName);
  const saved = await cache.match(url);
  if (saved) {
    await onProgress?.(asset.bytes);
    return saved;
  }
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 120_000);
  try {
    const response = await fetch(url, {
      cache: 'no-store',
      credentials: 'same-origin',
      redirect: 'error',
      signal: controller.signal,
    });
    if (!response.ok || response.status !== 200)
      throw new Error(`Could not download ${asset.path}: HTTP ${response.status}.`);
    const verified = await readVerifiedResponse(response, asset, onProgress);
    try {
      await cache.put(url, verified.clone());
    } catch (error) {
      if (required) throw error;
    }
    return verified;
  } finally {
    clearTimeout(timeout);
  }
}

async function prepareAll() {
  const total = release.assets.reduce((sum, asset) => sum + asset.bytes, 0);
  const report = progressReporter(total);
  let completed = 0;
  await report(0, 'progress', true);
  try {
    // One fetch at a time bounds transient Wasm response/hash buffers.
    for (const [url, asset] of assets) {
      await download(url, asset, true, (loaded) => report(completed + loaded));
      completed += asset.bytes;
      await report(completed, 'progress', true);
    }
    const status = await cachedStatus();
    if (!status.complete) throw new Error('Offline files disappeared while saving. Check available storage and retry.');
    await report(total, 'complete', true);
    return status;
  } catch (error) {
    await report(completed, 'failed', true);
    throw error;
  }
}

async function install() {
  try {
    // A tab may request its first solver or verifier after the host has already
    // replaced this release. Install only a complete, verified runtime.
    await prepareAll();
  } catch (error) {
    await caches.delete(cacheName);
    throw error;
  }
}

self.addEventListener('install', (event) => {
  event.waitUntil(install());
});
self.addEventListener('activate', (event) => {
  // No skipWaiting: upgrades activate only after the previous clients leave.
  // The initial bootstrap navigates before mounting the app. Never claim an
  // uncontrolled document: it may have loaded a different build during install.
  event.waitUntil(
    (async () => {
      for (const name of await caches.keys())
        if (name.startsWith(prefix) && name !== cacheName) await caches.delete(name);
    })(),
  );
});

self.addEventListener('fetch', (event) => {
  const request = event.request;
  if (request.method !== 'GET' || request.headers.has('range')) return;
  const url = new URL(request.url);
  if (url.origin !== new URL(scope).origin || !url.href.startsWith(scope)) return;
  // A controlled document always opens the index from its active release,
  // including while a newer release waits. Queries/fragments are not cached.
  if (
    request.mode === 'navigate' &&
    [new URL(scope).pathname, new URL('index.html', scope).pathname].includes(url.pathname)
  ) {
    event.respondWith(
      (async () => {
        const index = new URL('index.html', scope).href;
        return download(index, assets.get(index), false);
      })(),
    );
    return;
  }
  const asset = assets.get(url.href);
  if (asset) event.respondWith(download(url.href, asset, false));
});

self.addEventListener('message', (event) => {
  const port = event.ports[0];
  if (!port || !event.source?.id || !event.data || !['offline-status', 'offline-prepare'].includes(event.data.kind))
    return;
  event.waitUntil(
    (async () => {
      try {
        const client = await self.clients.get(event.source.id);
        if (!client || !client.url.startsWith(scope) || event.data.buildId !== release.buildId)
          throw new Error(
            'This tab and its offline worker belong to different builds. Close all app tabs and reopen the app.',
          );
        if (event.data.kind === 'offline-prepare') {
          preparation ??= prepareAll().finally(() => {
            preparation = undefined;
          });
          await preparation;
        }
        port.postMessage({ ok: true, buildId: release.buildId, status: await cachedStatus() });
      } catch (error) {
        port.postMessage({ ok: false, buildId: release.buildId, error: String(error).slice(0, 2048) });
      } finally {
        port.close();
      }
    })(),
  );
});
