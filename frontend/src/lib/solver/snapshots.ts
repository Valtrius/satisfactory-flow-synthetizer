import type { JobSnapshot } from '../../types';

const statuses = new Set(['running', 'cancelling', 'completed', 'cancelled', 'incomplete', 'unsat', 'failed']);

export function terminal(snapshot: JobSnapshot): boolean {
  return snapshot.status !== 'running' && snapshot.status !== 'cancelling';
}

/** Transport checks only. Rust decides feasibility, proof and enumeration status. */
export function assertSnapshot(value: unknown, jobId: string): asserts value is JobSnapshot {
  if (!value || typeof value !== 'object') throw new Error('Invalid browser job snapshot');
  const packet = value as JobSnapshot;
  if (
    packet.jobId !== jobId ||
    !statuses.has(packet.status) ||
    !Number.isSafeInteger(packet.sequence) ||
    packet.sequence! < 0 ||
    !Number.isSafeInteger(packet.startedAtMs) ||
    !Array.isArray(packet.results) ||
    (packet.result !== null && typeof packet.result !== 'object') ||
    typeof packet.enumerationComplete !== 'boolean' ||
    (packet.error !== null && typeof packet.error !== 'string')
  ) {
    throw new Error('Invalid browser job snapshot');
  }
  if (packet.resultsOmitted && packet.resultAppended) throw new Error('Conflicting browser snapshot flags');
  if (
    (packet.resultsOmitted || packet.resultAppended) &&
    (!Number.isSafeInteger(packet.resultsLen) || packet.resultsLen! < 0)
  )
    throw new Error('Invalid browser result count');
}

/** Materialize exactly the same thin/append wire contract used by HistoryQueue. */
export function materializeSnapshot(previous: JobSnapshot, packet: JobSnapshot): JobSnapshot {
  let results = packet.results;
  let result = packet.result;
  if (packet.resultsOmitted) {
    if (packet.resultsLen !== previous.results.length) throw new Error('Browser progress skipped a result packet');
    results = previous.results;
    result = previous.result;
  } else if (packet.resultAppended) {
    if (!packet.result || packet.resultsLen !== previous.results.length + 1)
      throw new Error('Browser append sequence is incomplete');
    results = [...previous.results, packet.result];
    result = previous.result ?? packet.result;
  }
  return { ...packet, result, results, resultsOmitted: false, resultAppended: false, resultsLen: 0 };
}

/** Admission can fail before Rust assets load. No solver evidence exists yet. */
export function emptySnapshot(jobId: string, startedAtMs: number): JobSnapshot {
  return {
    jobId,
    startedAtMs,
    status: 'running',
    progress: null,
    proof: null,
    sequence: 0,
    result: null,
    results: [],
    enumerationComplete: false,
    error: null,
  };
}
