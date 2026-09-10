import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { JobSnapshot, SolveRequest } from '../types';

export interface JobWatch {
  close: () => void;
}

function invokeError(error: unknown): Error {
  if (typeof error === 'string') {
    return new Error(error);
  }
  if (error instanceof Error) {
    return error;
  }
  return new Error(String(error));
}

export async function createJob(request: SolveRequest): Promise<string> {
  try {
    return await invoke<string>('create_job', { request });
  } catch (error) {
    throw invokeError(error);
  }
}

export async function getJob(jobId: string): Promise<JobSnapshot> {
  try {
    return await invoke<JobSnapshot>('get_job', { jobId });
  } catch (error) {
    throw invokeError(error);
  }
}

export async function watchJob(
  jobId: string,
  onSnapshot: (snapshot: JobSnapshot) => void,
  onError: () => void,
): Promise<JobWatch> {
  let closed = false;
  let unlisten: UnlistenFn | undefined;
  try {
    unlisten = await listen<JobSnapshot>('job-snapshot', (event) => {
      if (event.payload.jobId === jobId) {
        onSnapshot(event.payload);
      }
    });
    if (closed) {
      unlisten();
      return { close() {} };
    }
    onSnapshot(await getJob(jobId));
  } catch {
    onError();
  }
  return {
    close() {
      closed = true;
      unlisten?.();
      unlisten = undefined;
    },
  };
}

export async function cancelJob(jobId: string): Promise<JobSnapshot> {
  try {
    return await invoke<JobSnapshot>('cancel_job', { jobId });
  } catch (error) {
    throw invokeError(error);
  }
}

export async function releaseJob(jobId: string): Promise<void> {
  await invoke('release_job', { jobId });
}

export async function shutdownJobs(): Promise<JobSnapshot[]> {
  return invoke('shutdown_jobs');
}

export async function resumeJobs(): Promise<void> {
  await invoke('resume_jobs');
}
