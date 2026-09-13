import { expect, it } from 'vitest';
import { COORDINATOR_BATCH_SIZE, coordinatorQueueLimit, createCoordinatorInbox } from './inbox';
import type { Events } from './protocol';

const message = (index: number): Events => ({
  kind: 'events',
  protocol: 2,
  jobId: 'job',
  attempt: 'attempt',
  events: [{ kind: 'retired', id: String(index), verdict: 'cancelled', detail: '' }],
  receipts: [],
});

it.each([1, 8, 32, 64, 192])('bounds %s worker bursts while keeping collection checkpoints at sixteen', (workers) => {
  const inbox = createCoordinatorInbox(workers);
  const limit = coordinatorQueueLimit(workers);
  for (let index = 0; index < limit; index++) inbox.push(message(index));
  expect(() => inbox.push(message(limit))).toThrow('bounded budget');
  const delivered = [];
  while (inbox.length) {
    const batch = inbox.take();
    expect(batch.length).toBeLessThanOrEqual(COORDINATOR_BATCH_SIZE);
    delivered.push(...batch.map((value) => value.events[0].id));
  }
  expect(delivered).toEqual(Array.from({ length: limit }, (_, index) => String(index)));
});

it('rejects multi-event envelopes that would bypass the graph batch limit', () => {
  const inbox = createCoordinatorInbox(32);
  const value = message(0);
  expect(() => inbox.push({ ...value, events: [...value.events, ...value.events] })).toThrow('bounded budget');
  expect(inbox.length).toBe(0);
});
