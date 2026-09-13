import { layoutSolution, solutionLayoutKey } from './graph';
import type { CachedGraphLayout } from './historyModel';
import type { Solution } from '../types';

/** Prepare a selected graph without changing the displayed graph or saved selection. */
export async function prepareGraphSelection(
  load: () => Promise<Solution>,
  savedLayout: () => CachedGraphLayout | undefined,
  current: () => boolean,
): Promise<{ solution: Solution; layout: CachedGraphLayout } | null> {
  const solution = await load();
  if (!current()) return null;
  const layoutKey = solutionLayoutKey(solution);
  const cached = savedLayout();
  const layout =
    cached && cached.layoutKey === layoutKey && cached.nodes.length
      ? cached
      : { ...(await layoutSolution(solution)), layoutKey };
  return current() ? { solution, layout } : null;
}
