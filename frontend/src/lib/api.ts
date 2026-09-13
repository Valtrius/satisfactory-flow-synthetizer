import type { JobSnapshot, SolveRequest } from '../types';
import { getPlatform } from './platform';
export type { JobWatch } from './platform/contracts';

export const createJob = (request: SolveRequest) => getPlatform().jobs.create(request);
export const getJob = (jobId: string) => getPlatform().jobs.get(jobId);
export const watchJob = (jobId: string, onSnapshot: (snapshot: JobSnapshot) => void, onError: () => void) =>
  getPlatform().jobs.watch(jobId, onSnapshot, onError);
export const cancelJob = (jobId: string) => getPlatform().jobs.cancel(jobId);
export const releaseJob = (jobId: string) => getPlatform().jobs.release(jobId);
export const shutdownJobs = () => getPlatform().jobs.shutdown();
export const resumeJobs = () => getPlatform().jobs.resume();
