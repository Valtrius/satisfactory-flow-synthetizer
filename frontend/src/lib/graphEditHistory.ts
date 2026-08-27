import type { Edge, Node } from '@xyflow/svelte';

export type GraphSnapshot = {
  nodes: Node[];
  edges: Edge[];
};

const MAX_STACK = 50;

/** Strip interactive callbacks so snapshots stay serializable / restorable. */
export function cloneGraphNodes(nodes: Node[]): Node[] {
  return nodes.map((node) => {
    const data = { ...(node.data as Record<string, unknown>) };
    delete data.onSwapSides;
    delete data.onRotatePorts;
    if (Array.isArray(data.inputPositions)) data.inputPositions = [...data.inputPositions];
    if (Array.isArray(data.outputPositions)) data.outputPositions = [...data.outputPositions];
    return {
      ...node,
      position: { ...node.position },
      data,
    };
  });
}

export function cloneGraphEdges(edges: Edge[]): Edge[] {
  return edges.map((edge) => ({ ...edge }));
}

export function captureGraphSnapshot(nodes: Node[], edges: Edge[]): GraphSnapshot {
  return {
    nodes: cloneGraphNodes(nodes),
    edges: cloneGraphEdges(edges),
  };
}

export function pushGraphUndo(past: GraphSnapshot[], snapshot: GraphSnapshot): GraphSnapshot[] {
  const next = [...past, snapshot];
  if (next.length <= MAX_STACK) return next;
  return next.slice(next.length - MAX_STACK);
}

export function snapshotsEqual(left: GraphSnapshot, right: GraphSnapshot): boolean {
  return JSON.stringify(positionsAndPorts(left)) === JSON.stringify(positionsAndPorts(right));
}

function positionsAndPorts(snapshot: GraphSnapshot): unknown {
  return {
    nodes: snapshot.nodes.map((node) => ({
      id: node.id,
      position: node.position,
      inputPositions: (node.data as Record<string, unknown>).inputPositions ?? null,
      outputPositions: (node.data as Record<string, unknown>).outputPositions ?? null,
    })),
    edges: snapshot.edges.map((edge) => ({
      id: edge.id,
      source: edge.source,
      target: edge.target,
      sourceHandle: edge.sourceHandle,
      targetHandle: edge.targetHandle,
    })),
  };
}
