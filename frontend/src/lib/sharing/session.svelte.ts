import { onMount } from 'svelte';
import type { Node } from '@xyflow/svelte';
import { cloneGraphNodes } from '../graphEditHistory';
import type { Solution, SolveRequest } from '../../types';
import type { HistoryEntry } from '../historyModel';
import type { VerifiedShare } from './client';
import { sharedHistoryEntry } from './history';

type ShareDialogOptions = { target?: { request: SolveRequest; solution: Solution; nodes: Node[] }; source?: string };
type ShareHost = {
  canSave(): boolean;
  selectedEntry(): HistoryEntry | null;
  solution(): Solution | null;
  graphNodes(): Node[];
  entries(): HistoryEntry[];
  add(entry: HistoryEntry): void;
  select(id: string): Promise<void>;
  flushChrome(): void;
  flushHistory(): Promise<void>;
};

/** Own share previews, fragment navigation, and idempotent explicit saves. */
export function createShareSession(host: ShareHost) {
  let dialog = $state<ShareDialogOptions | null>(null);
  let revision = $state(0);
  const saved = new WeakMap<VerifiedShare, string>();
  const open = (options: ShareDialogOptions) => {
    dialog = options;
    revision++;
  };
  onMount(() => {
    const openFragment = () => {
      if (window.location.hash) open({ source: window.location.hash });
    };
    openFragment();
    window.addEventListener('hashchange', openFragment);
    return () => window.removeEventListener('hashchange', openFragment);
  });
  return {
    get dialog() {
      return dialog;
    },
    get revision() {
      return revision;
    },
    open,
    shareSelected() {
      const entry = host.selectedEntry(),
        solution = host.solution();
      if (entry && solution)
        open({ target: { request: entry.request, solution, nodes: cloneGraphNodes(host.graphNodes()) } });
    },
    async save(value: VerifiedShare) {
      if (!host.canSave()) throw new Error('History is not ready for saving.');
      host.flushChrome();
      let id = saved.get(value);
      if (!id || !host.entries().some((entry) => entry.id === id)) {
        const entry = await sharedHistoryEntry(value);
        id = entry.id;
        saved.set(value, id);
        host.add(entry);
      }
      await host.select(id);
      await host.flushHistory();
    },
    close() {
      dialog = null;
      if (window.location.hash)
        window.history.replaceState(null, '', window.location.pathname + window.location.search);
    },
  };
}
