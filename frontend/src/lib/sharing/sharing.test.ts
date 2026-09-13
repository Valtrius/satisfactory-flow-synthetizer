import { afterEach, expect, it, vi } from 'vitest';
import { inlineLink, readShareLocation, viewerBase, boundedText } from './codec';
import { runShareWorker } from './client';
import { publicViewerUrl } from './viewer';

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});

it('accepts only bounded selected-solution fragments and preserves the viewer project path', () => {
  expect(inlineLink('https://example.org/project/', 's1.YQ')).toBe('https://example.org/project/#s1.YQ');
  expect(readShareLocation('https://untrusted.invalid/other/#s1.YQ')).toEqual({ kind: 'inline', token: 's1.YQ' });
  expect(() => readShareLocation('#problem=abc')).toThrow();
  expect(() => readShareLocation('#share=' + 'a'.repeat(22))).toThrow();
  expect(() => inlineLink('https://example.org/project/', 'x'.repeat(32_768))).toThrow();
  expect(() => boundedText('é'.repeat(131_073))).toThrow();
  for (const invalid of [
    'javascript:alert(1)',
    'http://remote.invalid/',
    'https://user:pass@example.org/',
    'https://example.org/?x=y',
    'https://example.org/#hash',
  ])
    expect(() => viewerBase(invalid)).toThrow();
});

class FakeWorker {
  static last: FakeWorker;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onmessageerror: (() => void) | null = null;
  onerror: ((event: { message: string; preventDefault(): void }) => void) | null = null;
  terminate = vi.fn();
  postMessage = vi.fn();
  constructor() {
    FakeWorker.last = this;
  }
}

it('terminates an unresponsive worker at the deadline and ignores its late result', async () => {
  vi.useFakeTimers();
  vi.stubGlobal('Worker', FakeWorker);
  const pending = runShareWorker({ operation: 'open', source: '{}' });
  const rejected = expect(pending).rejects.toThrow('20-second');
  await vi.advanceTimersByTimeAsync(20_000);
  await rejected;
  FakeWorker.last.onmessage?.({ data: { kind: 'verified-share' } });
  expect(FakeWorker.last.terminate).toHaveBeenCalledOnce();
});

it('cancellation rejects rather than accepting an unverified solution', async () => {
  vi.stubGlobal('Worker', FakeWorker);
  const controller = new AbortController();
  const pending = runShareWorker({ operation: 'open', source: '{}' }, controller.signal);
  const rejected = expect(pending).rejects.toThrow('cancelled');
  controller.abort();
  await rejected;
  expect(FakeWorker.last.terminate).toHaveBeenCalledOnce();
});

it('uses the browser project directory without inventing a public desktop address', () => {
  vi.stubEnv('VITE_PUBLIC_APP_URL', '');
  expect(publicViewerUrl('browser')).toBe(new URL('./', document.baseURI).href);
  expect(publicViewerUrl('desktop')).toBeNull();
});

it('uses an explicitly configured public viewer on both hosts', () => {
  vi.stubEnv('VITE_PUBLIC_APP_URL', 'https://example.org/project/');
  expect(publicViewerUrl('browser')).toBe('https://example.org/project/');
  expect(publicViewerUrl('desktop')).toBe('https://example.org/project/');
});
