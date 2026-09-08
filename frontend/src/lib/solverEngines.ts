import type { SolverEngine } from '../types';

export const SOLVER_LABELS: Record<SolverEngine, string> = {
  custom: 'Custom',
  z3: 'Z3',
  astra: 'Astra',
};

export function parseSolverEngine(value: unknown): SolverEngine {
  return value === 'z3' || value === 'astra' ? value : 'custom';
}
