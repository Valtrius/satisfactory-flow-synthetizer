export const MAX_SHARE_BYTES = 262_144;
export const MAX_TOKEN_CHARS = 65_536;
export const MAX_LINK_CHARS = 32_768;

export function boundedText(text: string): Uint8Array<ArrayBuffer> {
  if (text.length > MAX_SHARE_BYTES) throw new Error('Selected solution exceeds 256 KiB.');
  const bytes = new TextEncoder().encode(text);
  if (bytes.length > MAX_SHARE_BYTES) throw new Error('Selected solution exceeds 256 KiB.');
  return bytes;
}

async function limited(stream: ReadableStream<Uint8Array>, max: number): Promise<Uint8Array<ArrayBuffer>> {
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      length += value.length;
      if (length > max) throw new Error('Decoded selected solution exceeds its size limit.');
      chunks.push(value);
    }
  } catch (error) {
    await reader.cancel().catch(() => {});
    throw error;
  } finally {
    reader.releaseLock();
  }
  const result = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }
  return result;
}

function base64url(bytes: Uint8Array): string {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '');
}

export async function encodeShare(text: string): Promise<string> {
  const bytes = boundedText(text);
  const compressed = await limited(new Blob([bytes]).stream().pipeThrough(new CompressionStream('gzip')), 49_149);
  const token = `s1.${base64url(compressed)}`;
  if (token.length > MAX_TOKEN_CHARS)
    throw new Error('Selected solution is too large for a share token. Export its JSON instead.');
  return token;
}

export async function decodeShare(token: string): Promise<string> {
  if (token.length > MAX_TOKEN_CHARS || !/^s1\.[A-Za-z0-9_-]+$/.test(token))
    throw new Error('Invalid or unsupported selected-solution token.');
  const encoded = token.slice(3);
  if (encoded.length % 4 === 1) throw new Error('Invalid share encoding.');
  const binary = atob(encoded.replaceAll('-', '+').replaceAll('_', '/'));
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
  if (base64url(bytes) !== encoded) throw new Error('Noncanonical share encoding.');
  let offset = 0;
  // Feed small compressed chunks so an expansion bomb cannot allocate its entire
  // output in one transform before the output limit is checked.
  const input = new ReadableStream<Uint8Array<ArrayBuffer>>(
    {
      pull(controller) {
        if (offset === bytes.length) {
          controller.close();
          return;
        }
        controller.enqueue(bytes.slice(offset, offset + 256));
        offset = Math.min(offset + 256, bytes.length);
      },
    },
    { highWaterMark: 0 },
  );
  const decoded = await limited(input.pipeThrough(new DecompressionStream('gzip')), MAX_SHARE_BYTES);
  return new TextDecoder('utf-8', { fatal: true }).decode(decoded);
}

export type ShareLocation = { kind: 'inline'; token: string };

export function readShareLocation(value: string): ShareLocation | null {
  if (value.length > MAX_TOKEN_CHARS + 4096) throw new Error('Share link is too long.');
  let fragment = value.trim();
  if (/^https?:\/\//.test(fragment)) fragment = new URL(fragment).hash;
  if (fragment.startsWith('#')) fragment = fragment.slice(1);
  if (!fragment) return null;
  if (fragment.startsWith('s1.')) {
    if (fragment.length > MAX_TOKEN_CHARS || !/^s1\.[A-Za-z0-9_-]+$/.test(fragment))
      throw new Error('Invalid share link.');
    return { kind: 'inline', token: fragment };
  }
  throw new Error('This is not a supported selected-solution link.');
}

export function viewerBase(value: string): string {
  const url = new URL(value);
  if (
    url.protocol !== 'https:' &&
    !(url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname))
  ) {
    throw new Error('Use an HTTPS viewer address or an HTTP loopback address for local testing.');
  }
  if (url.username || url.password || url.search || url.hash)
    throw new Error('The viewer address must not contain credentials, a query or a fragment.');
  if (!url.pathname.endsWith('/')) throw new Error('The viewer address must end with a slash.');
  return url.href;
}

export function inlineLink(base: string, token: string): string {
  const link = `${viewerBase(base)}#${token}`;
  if (link.length > MAX_LINK_CHARS)
    throw new Error('This solution exceeds the inline URL limit. Export its JSON instead.');
  return link;
}
