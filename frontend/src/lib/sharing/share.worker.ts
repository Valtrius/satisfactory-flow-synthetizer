import { boundedText, decodeShare, encodeShare } from './codec';
import type { ShareOperation } from './client';

type Verifier = {
  default(options: { module_or_path: URL }): Promise<unknown>;
  verifier_version(): number;
  verify_share_json(source: string): string;
  share_from_presentation_json(source: string): string;
};

self.onmessage = async ({ data }: MessageEvent<ShareOperation & { verifierBase: string }>) => {
  try {
    const source =
      data.operation === 'open' && data.source.startsWith('s1.') ? await decodeShare(data.source) : data.source;
    boundedText(source);
    const verifier: Verifier = await import(/* @vite-ignore */ new URL('solver_web.js', data.verifierBase).href);
    await verifier.default({ module_or_path: new URL('solver_web_bg.wasm', data.verifierBase) });
    if (verifier.verifier_version() !== 1) throw new Error('Incompatible verifier assets. Reload the application.');
    const response = JSON.parse(
      data.operation === 'create' ? verifier.share_from_presentation_json(source) : verifier.verify_share_json(source),
    );
    if (response.kind !== 'verified-share') throw new Error(response.error || 'Selected solution was rejected.');
    let token: string | null = null;
    try {
      token = await encodeShare(JSON.stringify(response.share));
    } catch {
      /* The bounded JSON export is still available. */
    }
    self.postMessage({ ...response, token });
  } catch (error) {
    self.postMessage({ kind: 'rejected', error: error instanceof Error ? error.message : String(error) });
  }
};
