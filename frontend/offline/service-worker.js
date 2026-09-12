/* This file is emitted with a build-specific manifest. It never executes solver work. */
const release = '__SFS_RELEASE_MANIFEST__';
const scope = self.registration.scope;
const prefix = `sfs-offline-v1:${scope}:`;
const cacheName = prefix + release.buildId;
const metadataUrl = new URL('__sfs_release__', scope).href;
const readyUrl = new URL('__sfs_complete__', scope).href;
const assets = new Map(release.assets.map((asset) => [new URL(asset.path, scope).href, asset]));
let preparation;

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
  if (complete) await cache.put(readyUrl, new Response('complete'));
  else await cache.delete(readyUrl);
  return { buildId: release.buildId, cached, total: assets.size, bytes, complete, shellComplete };
}

async function download(url, asset, required) {
  const cache = await caches.open(cacheName);
  const saved = await cache.match(url);
  if (saved) return saved;
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
    const body = await response.clone().arrayBuffer();
    const digest = [...new Uint8Array(await crypto.subtle.digest('SHA-256', body))]
      .map((byte) => byte.toString(16).padStart(2, '0'))
      .join('');
    if (body.byteLength !== asset.bytes || digest !== asset.sha256)
      throw new Error(`Version check failed for ${asset.path}. The previous offline build was kept.`);
    try {
      await cache.put(url, response.clone());
    } catch (error) {
      if (required) throw error;
    }
    return response;
  } finally {
    clearTimeout(timeout);
  }
}

async function prepareAll() {
  // One fetch at a time bounds transient Wasm response/hash buffers.
  for (const [url, asset] of assets) await download(url, asset, true);
  const status = await cachedStatus();
  if (!status.complete) throw new Error('Offline files disappeared while saving. Check available storage and retry.');
  return status;
}

async function install() {
  let retainFullOffline = false;
  for (const name of await caches.keys()) {
    if (name.startsWith(prefix) && name !== cacheName && (await (await caches.open(name)).match(readyUrl)))
      retainFullOffline = true;
  }
  try {
    const cache = await caches.open(cacheName);
    await cache.put(
      metadataUrl,
      new Response(JSON.stringify(release), { headers: { 'Content-Type': 'application/json' } }),
    );
    for (const [url, asset] of assets)
      if (retainFullOffline || asset.group === 'shell') await download(url, asset, true);
    await cachedStatus();
  } catch (error) {
    await caches.delete(cacheName);
    throw error;
  }
}

self.addEventListener('install', (event) => {
  event.waitUntil(install());
});
self.addEventListener('activate', (event) => {
  // No skipWaiting or clients.claim. An upgrade cannot replace an open page's
  // assets. The browser activates it only after the previous clients leave.
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
        port.postMessage({ ok: true, status: await cachedStatus() });
      } catch (error) {
        port.postMessage({ ok: false, error: String(error).slice(0, 2048) });
      } finally {
        port.close();
      }
    })(),
  );
});
