import type { EndpointInput, EndpointRow, SolveRequest, SolverEngine } from '../types';

export const MAX_ENDPOINTS = 24;

export type EndpointCollection = 'inputs' | 'outputs';

export function parseMultiplier(value: string): number {
  const parsed = Number.parseInt(value.trim(), 10);
  if (!Number.isFinite(parsed) || parsed < 1) return 1;
  return parsed;
}

export function clampMultiplier(value: string, max = MAX_ENDPOINTS): string {
  return String(Math.min(parseMultiplier(value), max));
}

export function endpointSlots(endpoints: EndpointRow[]): number {
  return endpoints.reduce((total, endpoint) => total + parseMultiplier(endpoint.multiplier), 0);
}

export function createEndpointRow(
  collection: EndpointCollection,
  id: number
): EndpointRow {
  return {
    id: `${collection === 'inputs' ? 'input' : 'output'}-${id}`,
    name: '',
    rate: '60',
    multiplier: '1'
  };
}

export function expandEndpoints(
  endpoints: EndpointRow[],
  max = MAX_ENDPOINTS
): EndpointInput[] {
  let remaining = max;
  return endpoints.flatMap((endpoint) => {
    const count = Math.min(parseMultiplier(endpoint.multiplier), Math.max(0, remaining));
    remaining -= count;
    if (count < 1) return [];
    return Array.from({ length: count }, (_, copyIndex) => ({
      id: count === 1 ? endpoint.id : `${endpoint.id}-${copyIndex + 1}`,
      name: endpoint.name,
      rate: endpoint.rate
    }));
  });
}

export function buildSolveRequest(
  inputs: EndpointRow[],
  outputs: EndpointRow[],
  beltRate: string,
  enumerateAllAtN: boolean,
  engine: SolverEngine = 'custom'
): SolveRequest {
  return {
    inputs: expandEndpoints(inputs),
    outputs: expandEndpoints(outputs),
    beltRate,
    enumerateAllAtN,
    engine
  };
}
