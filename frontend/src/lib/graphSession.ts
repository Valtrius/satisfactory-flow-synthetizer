import { tick } from 'svelte';
import { get, type Writable } from 'svelte/store';
import type { Edge, Node } from '@xyflow/svelte';
import { defaultSvgFileName, graphToSvg, saveSvgFile } from './exportSvg';
import {
  captureGraphSnapshot,
  cloneGraphEdges,
  cloneGraphNodes,
  pushGraphUndo,
  snapshotsEqual,
  type GraphSnapshot,
} from './graphEditHistory';
import {
  layoutSolution,
  rotateDevicePorts,
  rotateFlowGraph,
  solutionLayoutKey,
  swapDeviceSides,
  type PortSide,
  type RotateDirection,
} from './graph';
import type { CachedGraphLayout, HistoryEntry } from './historyModel';
import { DEFAULT_SORT_COLUMNS } from './solutionSort';
import { enumeratesLayouts, type EndpointRow, type Solution } from '../types';

export type GraphSessionHost = {
  nodes: Writable<Node[]>;
  edges: Writable<Edge[]>;
  getSelectedEntryId: () => string | null;
  getSelectedEntry: () => HistoryEntry | null;
  getSelectedSourceIndex: () => number;
  setSelectedSourceIndex: (index: number) => void;
  getSolution: () => Solution | null;
  setSolution: (solution: Solution | null) => void;
  getSolutions: () => Solution[];
  setSolutions: (solutions: Solution[]) => void;
  getSortColumns: () => HistoryEntry['sortColumns'];
  setSortColumns: (columns: HistoryEntry['sortColumns']) => void;
  getInputs: () => EndpointRow[];
  getOutputs: () => EndpointRow[];
  patchEntry: (id: string, patch: Partial<HistoryEntry>) => void;
  setError: (message: string) => void;
  /** Called when undo/redo availability or fit/fullscreen chrome changes. */
  onChromeChange?: (chrome: { fitRevision: number; fullscreen: boolean; canUndo: boolean; canRedo: boolean }) => void;
};

export type GraphSession = {
  fitRevision: number;
  fullscreen: boolean;
  canUndo: boolean;
  canRedo: boolean;
  clearView: () => void;
  flushChrome: () => void;
  onNodeDragStart: () => void;
  onNodeDragStop: () => void;
  rotate: (direction: RotateDirection) => void;
  undo: () => void;
  redo: () => void;
  resetLayout: () => Promise<void>;
  exportSvg: () => Promise<void>;
  setFullscreen: (expanded: boolean) => void;
  toggleFullscreen: () => void;
  selectSolution: (sourceIndex: number) => Promise<void>;
  applySolution: (next: Solution, options?: { force?: boolean }) => Promise<void>;
  hydrateFromEntry: (entry: HistoryEntry) => Promise<void>;
  syncLiveResults: (entry: HistoryEntry) => void;
  handleKeydown: (event: KeyboardEvent) => void;
};

/** Owns the displayed graph, its saved layout and pending layout operations. */
export function createGraphSession(host: GraphSessionHost): GraphSession {
  let fitRevision = 0;
  let fullscreen = false;
  let canUndo = false;
  let canRedo = false;

  let undoStack: GraphSnapshot[] = [];
  let redoStack: GraphSnapshot[] = [];
  let dragOrigin: GraphSnapshot | null = null;
  let layoutTicket = 0;
  let layoutKey = '';
  let committedSourceIndex: number | null = null;
  let committedEntryId: string | null = null;

  function beginLayout() {
    const ticket = ++layoutTicket;
    const entryId = host.getSelectedEntryId();
    const sourceIndex = host.getSelectedSourceIndex();
    return {
      entryId,
      sourceIndex,
      current: () =>
        ticket === layoutTicket &&
        entryId === host.getSelectedEntryId() &&
        sourceIndex === host.getSelectedSourceIndex(),
    };
  }

  const session: GraphSession = {
    get fitRevision() {
      return fitRevision;
    },
    get fullscreen() {
      return fullscreen;
    },
    get canUndo() {
      return canUndo;
    },
    get canRedo() {
      return canRedo;
    },
    clearView,
    flushChrome,
    onNodeDragStart,
    onNodeDragStop,
    rotate,
    undo,
    redo,
    resetLayout,
    exportSvg,
    setFullscreen,
    toggleFullscreen,
    selectSolution,
    applySolution,
    hydrateFromEntry,
    syncLiveResults,
    handleKeydown,
  };

  function notifyChrome(): void {
    host.onChromeChange?.({
      fitRevision,
      fullscreen,
      canUndo,
      canRedo,
    });
  }

  function syncEditFlags(): void {
    canUndo = undoStack.length > 0;
    canRedo = redoStack.length > 0;
    notifyChrome();
  }

  function clearEditHistory(): void {
    undoStack = [];
    redoStack = [];
    dragOrigin = null;
    syncEditFlags();
  }

  function bumpFit(): void {
    fitRevision += 1;
    notifyChrome();
  }

  function swapSides(nodeId: string, first: PortSide, second: PortSide): void {
    recordUndo();
    host.nodes.update((list) =>
      list.map((node) => {
        if (node.id !== nodeId) return node;
        const positions = swapDeviceSides(
          (node.data.inputPositions as PortSide[] | undefined) ?? [],
          (node.data.outputPositions as PortSide[] | undefined) ?? [],
          first,
          second,
        );
        return { ...node, data: { ...node.data, ...positions } };
      }),
    );
    persistLiveLayout();
  }

  function rotatePorts(nodeId: string, direction: RotateDirection): void {
    recordUndo();
    host.nodes.update((list) =>
      list.map((node) => {
        if (node.id !== nodeId) return node;
        const positions = rotateDevicePorts(
          (node.data.inputPositions as PortSide[] | undefined) ?? [],
          (node.data.outputPositions as PortSide[] | undefined) ?? [],
          direction,
        );
        return { ...node, data: { ...node.data, ...positions } };
      }),
    );
    persistLiveLayout();
  }

  function stampNodeCallbacks(list: Node[]): Node[] {
    return list.map((node) => {
      const { draggable: _draggable, selectable: _selectable, ...rest } = node;
      return {
        ...rest,
        data: {
          ...rest.data,
          onSwapSides: swapSides,
          onRotatePorts: rotatePorts,
        },
      };
    });
  }

  function layoutsForSelected(): Record<string, CachedGraphLayout> {
    const entry = host.getSelectedEntry();
    return entry ? { ...entry.layouts } : {};
  }

  function buildLayoutSnapshot(
    sourceIndex: number,
    key: string,
    nodeList: Node[],
    edgeList: Edge[],
  ): Record<string, CachedGraphLayout> {
    const layouts = layoutsForSelected();
    if (!key || nodeList.length === 0) return layouts;
    layouts[String(sourceIndex)] = {
      layoutKey: key,
      nodes: cloneGraphNodes(nodeList),
      edges: cloneGraphEdges(edgeList),
    };
    return layouts;
  }

  function rememberLayout(sourceIndex: number, key: string, nodeList: Node[], edgeList: Edge[]): void {
    const id = host.getSelectedEntryId();
    if (!id || !key || nodeList.length === 0) return;
    host.patchEntry(id, {
      layouts: buildLayoutSnapshot(sourceIndex, key, nodeList, edgeList),
    });
  }

  function persistLiveLayout(): void {
    if (
      committedEntryId !== host.getSelectedEntryId() ||
      committedSourceIndex == null ||
      !layoutKey ||
      get(host.nodes).length === 0
    )
      return;
    rememberLayout(committedSourceIndex, layoutKey, get(host.nodes), get(host.edges));
  }

  function flushChrome(): void {
    const id = host.getSelectedEntryId();
    const entry = host.getSelectedEntry();
    if (!id || !entry) return;
    let layouts = { ...entry.layouts };
    const sourceIndex = host.getSelectedSourceIndex();
    if (committedEntryId === id && committedSourceIndex === sourceIndex && layoutKey && get(host.nodes).length > 0) {
      layouts = buildLayoutSnapshot(sourceIndex, layoutKey, get(host.nodes), get(host.edges));
    }
    host.patchEntry(id, {
      selectedSourceIndex: sourceIndex,
      sortColumns: host.getSortColumns().map((column) => ({ ...column })),
      layouts,
    });
  }

  function recordUndo(): void {
    layoutTicket += 1;
    const snapshot = captureGraphSnapshot(get(host.nodes), get(host.edges));
    if (snapshot.nodes.length === 0) return;
    undoStack = pushGraphUndo(undoStack, snapshot);
    redoStack = [];
    syncEditFlags();
  }

  function applySnapshot(snapshot: GraphSnapshot): void {
    const nextNodes = stampNodeCallbacks(
      snapshot.nodes.map((node) => ({
        ...node,
        position: { ...node.position },
        data: { ...(node.data as Record<string, unknown>) },
      })),
    );
    host.nodes.set(nextNodes);
    host.edges.set(cloneGraphEdges(snapshot.edges));
    if (committedSourceIndex != null) {
      rememberLayout(committedSourceIndex, layoutKey, nextNodes, snapshot.edges);
    }
  }

  function undo(): void {
    if (undoStack.length === 0) return;
    const current = captureGraphSnapshot(get(host.nodes), get(host.edges));
    const previous = undoStack[undoStack.length - 1];
    undoStack = undoStack.slice(0, -1);
    redoStack = pushGraphUndo(redoStack, current);
    applySnapshot(previous);
    syncEditFlags();
  }

  function redo(): void {
    if (redoStack.length === 0) return;
    const current = captureGraphSnapshot(get(host.nodes), get(host.edges));
    const next = redoStack[redoStack.length - 1];
    redoStack = redoStack.slice(0, -1);
    undoStack = pushGraphUndo(undoStack, current);
    applySnapshot(next);
    syncEditFlags();
  }

  async function replaceGraph(nextNodes: Node[], nextEdges: Edge[], current: () => boolean): Promise<boolean> {
    if (!current()) return false;
    committedSourceIndex = null;
    committedEntryId = null;
    host.nodes.set([]);
    host.edges.set([]);
    await tick();
    if (!current()) return false;
    host.nodes.set(nextNodes);
    await tick();
    if (!current()) return false;
    host.edges.set(nextEdges);
    return true;
  }

  async function restoreLayout(next: Solution, sourceIndex: number, cached: CachedGraphLayout): Promise<void> {
    host.setSolution(next);
    const operation = beginLayout();
    const nextNodes = stampNodeCallbacks(cloneGraphNodes(cached.nodes));
    const nextEdges = cloneGraphEdges(cached.edges);
    if (!(await replaceGraph(nextNodes, nextEdges, operation.current)) || !operation.current()) return;
    layoutKey = cached.layoutKey;
    committedSourceIndex = sourceIndex;
    committedEntryId = operation.entryId;
    bumpFit();
    rememberLayout(sourceIndex, cached.layoutKey, nextNodes, nextEdges);
    clearEditHistory();
  }

  async function applySolution(next: Solution, options: { force?: boolean } = {}): Promise<void> {
    host.setSolution(next);
    const nextLayoutKey = solutionLayoutKey(next);
    if (
      !options.force &&
      committedEntryId === host.getSelectedEntryId() &&
      nextLayoutKey === layoutKey &&
      committedSourceIndex === host.getSelectedSourceIndex()
    ) {
      return;
    }
    const operation = beginLayout();
    try {
      const laidOut = await layoutSolution(next);
      if (!operation.current()) return;
      const nextNodes = stampNodeCallbacks(laidOut.nodes);
      if (!(await replaceGraph(nextNodes, laidOut.edges, operation.current)) || !operation.current()) return;
      layoutKey = nextLayoutKey;
      committedSourceIndex = operation.sourceIndex;
      committedEntryId = operation.entryId;
      bumpFit();
      rememberLayout(operation.sourceIndex, nextLayoutKey, nextNodes, laidOut.edges);
      clearEditHistory();
    } catch (error) {
      if (!operation.current()) return;
      host.setError(
        `The result is valid, but its graph could not be laid out: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
    }
  }

  async function resetLayout(): Promise<void> {
    const solution = host.getSolution();
    if (!solution || get(host.nodes).length === 0) return;
    const operation = beginLayout();
    try {
      const laidOut = await layoutSolution(solution);
      if (!operation.current()) return;
      recordUndo();
      const nextNodes = stampNodeCallbacks(laidOut.nodes);
      host.nodes.set(nextNodes);
      host.edges.set(laidOut.edges);
      layoutKey = solutionLayoutKey(solution);
      if (committedSourceIndex != null) {
        rememberLayout(committedSourceIndex, layoutKey, nextNodes, laidOut.edges);
      }
      bumpFit();
    } catch (error) {
      if (!operation.current()) return;
      host.setError(`The graph could not be reset: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  function onNodeDragStart(): void {
    layoutTicket += 1;
    dragOrigin = captureGraphSnapshot(get(host.nodes), get(host.edges));
  }

  function onNodeDragStop(): void {
    if (!dragOrigin) return;
    const current = captureGraphSnapshot(get(host.nodes), get(host.edges));
    if (!snapshotsEqual(dragOrigin, current)) {
      undoStack = pushGraphUndo(undoStack, dragOrigin);
      redoStack = [];
      syncEditFlags();
      persistLiveLayout();
    }
    dragOrigin = null;
  }

  function rotate(direction: RotateDirection): void {
    recordUndo();
    const rotated = rotateFlowGraph(get(host.nodes), get(host.edges), direction);
    host.nodes.set(rotated.nodes);
    host.edges.set(rotated.edges);
    bumpFit();
    persistLiveLayout();
  }

  async function exportSvg(): Promise<void> {
    const nodeList = get(host.nodes);
    const solution = host.getSolution();
    if (nodeList.length === 0 || !solution) return;
    try {
      const svg = graphToSvg(nodeList, get(host.edges));
      const inputs = host.getInputs();
      const outputs = host.getOutputs();
      const exportInputs =
        inputs.length > 0
          ? inputs
          : [
              {
                id: 'auto',
                name: '',
                rate: solution.totalInput.exact,
                multiplier: '1',
              },
            ];
      await saveSvgFile(svg, defaultSvgFileName(exportInputs, outputs, solution.stats.nodeCount));
    } catch (error) {
      host.setError(
        `The factory graph could not be exported: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  function setFullscreen(expanded: boolean): void {
    if (fullscreen === expanded) return;
    fullscreen = expanded;
    bumpFit();
    document.body.classList.toggle('graph-expanded', expanded);
    requestAnimationFrame(() => window.dispatchEvent(new Event('resize')));
    notifyChrome();
  }

  function toggleFullscreen(): void {
    setFullscreen(!fullscreen);
  }

  function snapshotCurrentLayout(sourceIndex: number): void {
    if (committedSourceIndex !== sourceIndex) return;
    rememberLayout(sourceIndex, layoutKey, get(host.nodes), get(host.edges));
  }

  async function selectSolution(sourceIndex: number): Promise<void> {
    const solutions = host.getSolutions();
    const next = solutions[sourceIndex];
    if (!next) return;
    const selectedSourceIndex = host.getSelectedSourceIndex();
    if (sourceIndex === selectedSourceIndex) {
      if (committedSourceIndex === sourceIndex && layoutKey === solutionLayoutKey(next) && get(host.nodes).length > 0) {
        return;
      }
      await applySolution(next, { force: true });
      return;
    }
    snapshotCurrentLayout(selectedSourceIndex);
    host.setSelectedSourceIndex(sourceIndex);
    const id = host.getSelectedEntryId();
    if (id) host.patchEntry(id, { selectedSourceIndex: sourceIndex });
    const nextKey = solutionLayoutKey(next);
    const cached = layoutsForSelected()[String(sourceIndex)];
    if (cached && cached.layoutKey === nextKey && cached.nodes.length > 0) {
      await restoreLayout(next, sourceIndex, cached);
      return;
    }
    await applySolution(next, { force: true });
  }

  function clearView(): void {
    layoutTicket += 1;
    layoutKey = '';
    committedSourceIndex = null;
    committedEntryId = null;
    host.setSolution(null);
    host.setSolutions([]);
    host.setSelectedSourceIndex(0);
    host.nodes.set([]);
    host.edges.set([]);
    clearEditHistory();
  }

  async function hydrateFromEntry(entry: HistoryEntry): Promise<void> {
    clearView();
    host.setSortColumns(
      entry.sortColumns.length > 0 ? entry.sortColumns.map((column) => ({ ...column })) : [...DEFAULT_SORT_COLUMNS],
    );
    const sourceIndex = Math.max(0, Math.min(entry.selectedSourceIndex, entry.results.length - 1));
    host.setSelectedSourceIndex(sourceIndex);
    const enumerate = enumeratesLayouts(entry.request.solveMode);
    if (enumerate) {
      host.setSolutions(entry.results);
      const chosen = entry.results[sourceIndex] ?? null;
      host.setSolution(chosen);
      if (chosen) {
        const cached = entry.layouts[String(sourceIndex)];
        const key = solutionLayoutKey(chosen);
        if (cached && cached.layoutKey === key && cached.nodes.length > 0) {
          await restoreLayout(chosen, sourceIndex, cached);
        } else {
          await applySolution(chosen, { force: true });
        }
      } else {
        host.nodes.set([]);
        host.edges.set([]);
        layoutKey = '';
        committedSourceIndex = null;
      }
    } else if (entry.result) {
      host.setSolutions([]);
      host.setSolution(entry.result);
      const cached = entry.layouts['0'];
      const key = solutionLayoutKey(entry.result);
      host.setSelectedSourceIndex(0);
      if (cached && cached.layoutKey === key && cached.nodes.length > 0) {
        await restoreLayout(entry.result, 0, cached);
      } else {
        await applySolution(entry.result, { force: true });
      }
    } else {
      host.setSolution(null);
      host.setSolutions([]);
      host.nodes.set([]);
      host.edges.set([]);
      layoutKey = '';
      committedSourceIndex = null;
    }
  }

  function syncLiveResults(entry: HistoryEntry): void {
    if (enumeratesLayouts(entry.request.solveMode)) {
      const prior = host.getSolutions();
      const had = prior.length > 0;
      host.setSolutions(entry.results);
      if (!had && entry.results[0]) {
        host.setSelectedSourceIndex(0);
        void applySolution(entry.results[0]);
      } else if (host.getSelectedSourceIndex() >= entry.results.length && entry.results[0]) {
        host.setSelectedSourceIndex(0);
        void applySolution(entry.results[0]);
      }
    } else if (entry.result) {
      host.setSolutions([]);
      void applySolution(entry.result);
    }
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && fullscreen) setFullscreen(false);

    const target = event.target;
    if (
      target instanceof HTMLElement &&
      (target.isContentEditable ||
        target.tagName === 'INPUT' ||
        target.tagName === 'TEXTAREA' ||
        target.tagName === 'SELECT')
    ) {
      return;
    }

    const mod = event.ctrlKey || event.metaKey;
    if (!mod || event.altKey) return;

    const key = event.key.toLowerCase();
    if (key === 'z' && !event.shiftKey) {
      if (undoStack.length === 0) return;
      event.preventDefault();
      undo();
      return;
    }
    if (key === 'y' || (key === 'z' && event.shiftKey)) {
      if (redoStack.length === 0) return;
      event.preventDefault();
      redo();
    }
  }

  return session;
}
