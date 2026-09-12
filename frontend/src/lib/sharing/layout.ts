import type { Node, Position } from '@xyflow/svelte';
import type { Solution } from '../../types';
import type { FlowGraph, PortSide } from '../graph';

export type ShareLayout = {
  version: 1;
  nodes: {
    id: string;
    x: number;
    y: number;
    sourcePosition: PortSide;
    targetPosition: PortSide;
    inputPositions: PortSide[];
    outputPositions: PortSide[];
  }[];
};

/** Match the verifier's graph-local operator and discard IDs; endpoints retain their IDs. */
export function captureShareLayout(solution: Solution, nodes: Node[]): ShareLayout {
  const displayed = new Map(nodes.map((node) => [node.id, node]));
  let operator = 0;
  let discard = 0;
  return {
    version: 1,
    nodes: solution.nodes.map((node) => {
      const view = displayed.get(node.id);
      if (!view) throw new Error('Wait for the topology graph to finish loading before sharing.');
      const id =
        node.kind === 'discard'
          ? `sink-${discard++}`
          : node.kind === 'input' || node.kind === 'output'
            ? node.id
            : `operator-${operator++}`;
      return {
        id,
        x: view.position.x,
        y: view.position.y,
        sourcePosition: view.sourcePosition ?? 'right',
        targetPosition: view.targetPosition ?? 'left',
        inputPositions: [...((view.data.inputPositions as PortSide[]) ?? [])],
        outputPositions: [...((view.data.outputPositions as PortSide[]) ?? [])],
      };
    }),
  };
}

/** Accept only coordinates and port sides for the independently verified nodes. */
export function readShareLayout(value: unknown, solution: Solution): ShareLayout {
  const invalid = () => {
    throw new Error('Invalid graph positions in the solution JSON.');
  };
  if (!value || typeof value !== 'object') return invalid();
  const layout = value as ShareLayout;
  if (layout.version !== 1 || !Array.isArray(layout.nodes) || layout.nodes.length !== solution.nodes.length)
    return invalid();
  const remaining = new Map(solution.nodes.map((node) => [node.id, node]));
  const sides = ['left', 'right', 'top', 'bottom'];
  return {
    version: 1,
    nodes: layout.nodes.map((node) => {
      const physical = node && remaining.get(node.id);
      if (!physical) return invalid();
      remaining.delete(node.id);
      const inputs = physical.kind.startsWith('merger')
        ? Number(physical.kind.at(-1))
        : physical.kind === 'input'
          ? 0
          : 1;
      const outputs = physical.kind.startsWith('splitter')
        ? Number(physical.kind.at(-1))
        : physical.kind === 'input' || physical.kind.startsWith('merger')
          ? 1
          : 0;
      if (
        !Number.isFinite(node.x) ||
        !Number.isFinite(node.y) ||
        Math.abs(node.x) > 1e7 ||
        Math.abs(node.y) > 1e7 ||
        !sides.includes(node.sourcePosition) ||
        !sides.includes(node.targetPosition) ||
        !Array.isArray(node.inputPositions) ||
        node.inputPositions.length !== inputs ||
        !Array.isArray(node.outputPositions) ||
        node.outputPositions.length !== outputs
      )
        return invalid();
      const ports = [...node.inputPositions, ...node.outputPositions];
      if (ports.some((side) => !sides.includes(side)) || new Set(ports).size !== ports.length) return invalid();
      return {
        id: node.id,
        x: node.x,
        y: node.y,
        sourcePosition: node.sourcePosition,
        targetPosition: node.targetPosition,
        inputPositions: [...node.inputPositions],
        outputPositions: [...node.outputPositions],
      };
    }),
  };
}

export function applyShareLayout(graph: FlowGraph, layout?: ShareLayout): FlowGraph {
  if (!layout) return graph;
  const positions = new Map(layout.nodes.map((node) => [node.id, node]));
  return {
    ...graph,
    nodes: graph.nodes.map((node) => {
      const saved = positions.get(node.id)!;
      return {
        ...node,
        position: { x: saved.x, y: saved.y },
        sourcePosition: saved.sourcePosition as Position,
        targetPosition: saved.targetPosition as Position,
        data: { ...node.data, inputPositions: [...saved.inputPositions], outputPositions: [...saved.outputPositions] },
      };
    }),
  };
}
