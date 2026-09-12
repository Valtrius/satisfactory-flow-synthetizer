import { expect, it, vi } from 'vitest';
import { createPagedResults } from './pagedResults';
import type { CollectionStore } from './platform/contracts';
import type { SolutionPage } from '../types';

it('coalesces replacement requests and rejects stale page results and errors', async () => {
  const requests: { resolve(value: SolutionPage): void; reject(error: Error): void }[] = [];
  const page = vi.fn(() => new Promise<SolutionPage>((resolve, reject) => requests.push({ resolve, reject })));
  const receive = vi.fn(),
    error = vi.fn();
  const controller = createPagedResults({ page } as unknown as CollectionStore, receive, error);
  const ref = { version: 1 as const, id: 'collection', count: 200, preferredIndex: 0 };
  controller.load(ref, 0, 64, []);
  controller.load(ref, 64, 64, []);
  controller.load(ref, 128, 64, []);
  expect(page).toHaveBeenCalledOnce();
  requests[0].resolve({ offset: 0, total: 200, rows: [] });
  await vi.waitFor(() => expect(page).toHaveBeenCalledTimes(2));
  expect(page.mock.calls[1]).toEqual([ref, 128, 64, []]);
  expect(receive).not.toHaveBeenCalled();
  controller.clear();
  requests[1].reject(new Error('stale read'));
  await Promise.resolve();
  await Promise.resolve();
  expect(error).not.toHaveBeenCalled();
  controller.load(ref, 64, 64, []);
  await vi.waitFor(() => expect(page).toHaveBeenCalledTimes(3));
  requests[2].resolve({ offset: 64, total: 200, rows: [] });
  await vi.waitFor(() => expect(receive).toHaveBeenCalledWith({ offset: 64, total: 200, rows: [] }));
});
