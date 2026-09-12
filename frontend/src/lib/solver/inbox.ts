import type { Events } from './protocol';

export const COORDINATOR_BATCH_SIZE = 16;

/** Each leaf can have one witness and its subsequent forced retirement queued. */
export function coordinatorQueueLimit(workers: number): number {
  return Math.max(COORDINATOR_BATCH_SIZE, workers * 2);
}

export function createCoordinatorInbox(workers: number) {
  const queue: Events[] = [];
  return {
    get length() {
      return queue.length;
    },
    push(message: Events): void {
      if (
        queue.length >= coordinatorQueueLimit(workers) ||
        !Array.isArray(message.events) ||
        message.events.length !== 1 ||
        !Array.isArray(message.receipts) ||
        message.receipts.length > 1
      ) {
        throw new Error('Browser event queue exceeded its bounded budget.');
      }
      queue.push(message);
    },
    take(): Events[] {
      return queue.splice(0, COORDINATOR_BATCH_SIZE);
    },
  };
}
