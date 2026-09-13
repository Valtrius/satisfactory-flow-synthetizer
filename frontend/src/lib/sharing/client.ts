import type { Solution, SolveRequest } from '../../types';
import { boundedText } from './codec';
import type { ShareLayout } from './layout';

export interface SelectedShare {
  kind: 'selected-solution';
  version: 1;
  request: { inputs: { name: string; rate: string }[]; outputs: { name: string; rate: string }[]; beltRate: string };
  topology: { nodes: string[]; links: unknown[] };
}

export interface VerifiedShare {
  share: SelectedShare;
  solution: Solution;
  token: string | null;
  layout?: ShareLayout;
}
export type ShareOperation = { operation: 'open'; source: string } | { operation: 'create'; source: string };

export function runShareWorker(operation: ShareOperation, signal?: AbortSignal): Promise<VerifiedShare> {
  boundedText(operation.source);
  if (signal?.aborted) return Promise.reject(new DOMException('Share operation cancelled.', 'AbortError'));
  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL('./share.worker.ts', import.meta.url), { type: 'module' });
    let settled = false;
    const finish = (error?: unknown, result?: VerifiedShare) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      signal?.removeEventListener('abort', cancel);
      worker.terminate();
      if (error) reject(error);
      else resolve(result!);
    };
    const cancel = () => finish(new DOMException('Share operation cancelled.', 'AbortError'));
    const timer = setTimeout(
      () => finish(new Error('Selected-solution verification exceeded its 20-second limit. No solution was imported.')),
      20_000,
    );
    signal?.addEventListener('abort', cancel, { once: true });
    worker.onerror = (event) => {
      event.preventDefault();
      finish(new Error(event.message || 'The verification worker failed.'));
    };
    worker.onmessageerror = () => finish(new Error('The verification worker returned an unreadable message.'));
    worker.onmessage = ({ data }) => {
      if (data.kind === 'rejected') finish(new Error(data.error));
      else if (data.kind === 'verified-share') finish(undefined, data);
      else finish(new Error('Unexpected verification response.'));
    };
    const verifierBase = new URL(`${import.meta.env.BASE_URL}${__SFS_VERIFIER_DIR__}/`, document.baseURI).href;
    try {
      worker.postMessage({ ...operation, verifierBase });
    } catch (error) {
      finish(error);
    }
  });
}

export function createSelectedShare(
  request: SolveRequest,
  solution: Solution,
  signal?: AbortSignal,
): Promise<VerifiedShare> {
  if (solution.nodes.length > 1328 || solution.edges.length > 1024)
    return Promise.reject(new Error('Selected solution exceeds sharing limits.'));
  const endpoints = (values: SolveRequest['inputs']) => values.map(({ name, rate }) => ({ name, rate }));
  const source = JSON.stringify({
    request: { inputs: endpoints(request.inputs), outputs: endpoints(request.outputs), beltRate: request.beltRate },
    solution: {
      modelVersion: solution.modelVersion,
      nodes: solution.nodes.map(({ id, kind }) => ({ id, kind })),
      edges: solution.edges.map(({ id, source, target, sourcePort, targetPort, rate }) => ({
        id,
        source,
        target,
        sourcePort,
        targetPort,
        rate: { exact: rate.exact },
      })),
    },
  });
  return runShareWorker({ operation: 'create', source }, signal);
}
