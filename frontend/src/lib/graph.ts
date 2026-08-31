import type { Edge, Node } from '@xyflow/svelte';
import { MarkerType, Position } from '@xyflow/svelte';
import type { Solution } from '../types';

const endpointHeight = 76;
/** Horizontal padding from `.svelte-flow__node` (`px-4`). */
const endpointPaddingX = 32;
const endpointFont = '700 16px Inter, ui-sans-serif, system-ui, sans-serif';
const endpointFallbackCharWidth = 9.6;
const deviceSize = 76;

/** Shared snap pitch for initial layout and interactive drag. */
export const GRAPH_SNAP_GRID = 24;

export function snapPositionToGrid(
  position: { x: number; y: number },
  grid: number = GRAPH_SNAP_GRID,
): { x: number; y: number } {
  return {
    x: Math.round(position.x / grid) * grid,
    y: Math.round(position.y / grid) * grid,
  };
}

export type PortSide = 'left' | 'right' | 'top' | 'bottom';

export const PORT_SIDES: readonly PortSide[] = ['top', 'right', 'bottom', 'left'];

export interface NodeBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface PortConnection {
  port: number;
  other: { x: number; y: number };
}

interface PortAssignments {
  inputs: Map<string, PortSide[]>;
  outputs: Map<string, PortSide[]>;
}

export interface FlowGraph {
  nodes: Node[];
  edges: Edge[];
}

/** Stable key for layout cache invalidation when the solution topology changes. */
export function solutionLayoutKey(next: Solution): string {
  return [
    next.engine,
    next.stats.nodeCount,
    next.stats.linkCount ?? next.stats.beltCount ?? 0,
    next.stats.feedbackLoops,
    next.stats.internalMaxThroughput?.exact ?? '',
    next.nodes.length,
    next.edges.map((edge) => `${edge.source}:${edge.target}:${edge.rate.exact}`).join('|'),
  ].join('::');
}

export interface PortReference {
  direction: 'input' | 'output';
  port: number;
}

export function occupantAtSide(
  inputPositions: PortSide[],
  outputPositions: PortSide[],
  side: PortSide,
): PortReference | null {
  const inputPort = inputPositions.indexOf(side);
  if (inputPort >= 0) return { direction: 'input', port: inputPort };
  const outputPort = outputPositions.indexOf(side);
  if (outputPort >= 0) return { direction: 'output', port: outputPort };
  return null;
}

function setPortSide(
  inputPositions: PortSide[],
  outputPositions: PortSide[],
  occupant: PortReference,
  side: PortSide,
): void {
  const positions = occupant.direction === 'input' ? inputPositions : outputPositions;
  positions[occupant.port] = side;
}

export function swapDeviceSides(
  inputPositions: PortSide[],
  outputPositions: PortSide[],
  first: PortSide,
  second: PortSide,
): { inputPositions: PortSide[]; outputPositions: PortSide[] } {
  const inputs = [...inputPositions];
  const outputs = [...outputPositions];
  const firstOccupant = occupantAtSide(inputs, outputs, first);
  const secondOccupant = occupantAtSide(inputs, outputs, second);
  if (firstOccupant && secondOccupant) {
    setPortSide(inputs, outputs, firstOccupant, second);
    setPortSide(inputs, outputs, secondOccupant, first);
  } else if (firstOccupant) {
    setPortSide(inputs, outputs, firstOccupant, second);
  } else if (secondOccupant) {
    setPortSide(inputs, outputs, secondOccupant, first);
  }
  return { inputPositions: inputs, outputPositions: outputs };
}

export interface Point {
  x: number;
  y: number;
}

export function handlePoint(bounds: NodeBounds, side: PortSide): Point {
  if (side === 'left') return { x: bounds.x, y: bounds.y + bounds.height / 2 };
  if (side === 'right') return { x: bounds.x + bounds.width, y: bounds.y + bounds.height / 2 };
  if (side === 'top') return { x: bounds.x + bounds.width / 2, y: bounds.y };
  return { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height };
}

export function nodeBox(node: Node): NodeBounds {
  const device = node.type === 'factory';
  const fallback = device ? deviceSize : endpointHeight;
  const width = node.measured?.width ?? node.width ?? fallback;
  const height = node.measured?.height ?? node.height ?? fallback;
  return {
    x: node.position.x,
    y: node.position.y,
    width,
    height,
  };
}

function positionToSide(position: Position | undefined, fallback: PortSide): PortSide {
  if (position === Position.Top) return 'top';
  if (position === Position.Bottom) return 'bottom';
  if (position === Position.Left) return 'left';
  if (position === Position.Right) return 'right';
  return fallback;
}

function portSideToElk(side: PortSide): 'NORTH' | 'EAST' | 'SOUTH' | 'WEST' {
  if (side === 'top') return 'NORTH';
  if (side === 'right') return 'EAST';
  if (side === 'bottom') return 'SOUTH';
  return 'WEST';
}

export type RotateDirection = 'cw' | 'ccw';

export function rotatePortSide(side: PortSide, direction: RotateDirection): PortSide {
  const index = PORT_SIDES.indexOf(side);
  return PORT_SIDES[(index + (direction === 'cw' ? 1 : 3)) % PORT_SIDES.length];
}

export function rotatePoint(point: Point, origin: Point, direction: RotateDirection): Point {
  const dx = point.x - origin.x;
  const dy = point.y - origin.y;
  return direction === 'cw' ? { x: origin.x - dy, y: origin.y + dx } : { x: origin.x + dy, y: origin.y - dx };
}

export function sideToPosition(side: PortSide): Position {
  if (side === 'top') return Position.Top;
  if (side === 'bottom') return Position.Bottom;
  if (side === 'left') return Position.Left;
  return Position.Right;
}

function parseHandleIndex(handleId: string | null | undefined, prefix: string): number {
  if (!handleId?.startsWith(prefix)) return 0;
  const parsed = Number.parseInt(handleId.slice(prefix.length), 10);
  return Number.isFinite(parsed) ? parsed : 0;
}

export function handleAnchor(
  node: Node,
  direction: 'source' | 'target',
  handleId?: string | null,
): { point: Point; side: PortSide } {
  const box = nodeBox(node);
  const positions =
    direction === 'source'
      ? ((node.data.outputPositions as PortSide[] | undefined) ?? [])
      : ((node.data.inputPositions as PortSide[] | undefined) ?? []);
  const prefix = direction === 'source' ? 'source-' : 'target-';
  const index = parseHandleIndex(handleId, prefix);
  const fallback: PortSide =
    direction === 'source' ? positionToSide(node.sourcePosition, 'right') : positionToSide(node.targetPosition, 'left');
  const side = positions[index] ?? fallback;
  return { point: handlePoint(box, side), side };
}

export function rotateDevicePorts(
  inputPositions: PortSide[],
  outputPositions: PortSide[],
  direction: RotateDirection,
): { inputPositions: PortSide[]; outputPositions: PortSide[] } {
  return {
    inputPositions: inputPositions.map((side) => rotatePortSide(side, direction)),
    outputPositions: outputPositions.map((side) => rotatePortSide(side, direction)),
  };
}

export function rotateFlowGraph(
  nodes: Node[],
  edges: Edge[],
  direction: RotateDirection,
): { nodes: Node[]; edges: Edge[] } {
  if (nodes.length === 0) return { nodes, edges };
  const boxes = nodes.map(nodeBox);
  const centers = boxes.map((box) => ({
    x: box.x + box.width / 2,
    y: box.y + box.height / 2,
  }));
  const minX = Math.min(...centers.map((center) => center.x));
  const minY = Math.min(...centers.map((center) => center.y));
  const maxX = Math.max(...centers.map((center) => center.x));
  const maxY = Math.max(...centers.map((center) => center.y));
  const origin = { x: (minX + maxX) / 2, y: (minY + maxY) / 2 };
  return {
    nodes: nodes.map((node, index) => {
      const box = boxes[index];
      const center = rotatePoint(centers[index], origin, direction);
      const inputPositions = ((node.data.inputPositions as PortSide[] | undefined) ?? []).map((side) =>
        rotatePortSide(side, direction),
      );
      const outputPositions = ((node.data.outputPositions as PortSide[] | undefined) ?? []).map((side) =>
        rotatePortSide(side, direction),
      );
      return {
        ...node,
        position: { x: center.x - box.width / 2, y: center.y - box.height / 2 },
        sourcePosition: sideToPosition(rotatePortSide(positionToSide(node.sourcePosition, 'right'), direction)),
        targetPosition: sideToPosition(rotatePortSide(positionToSide(node.targetPosition, 'left'), direction)),
        data: { ...node.data, inputPositions, outputPositions },
      };
    }),
    edges,
  };
}

function endpointLabel(label: string): string {
  const stripped = label.replace(/\s*\/min\b/gi, '').trim();
  const separator = stripped.lastIndexOf('·');
  return (separator >= 0 ? stripped.slice(separator + 1) : stripped).trim();
}

function nodeClass(kind: string): string {
  return `factory-node factory-node--${kind}`;
}

function isSplitter(kind: string): boolean {
  return kind === 'splitter2' || kind === 'splitter3';
}

function isMerger(kind: string): boolean {
  return kind === 'merger2' || kind === 'merger3';
}

function isDevice(kind: string): boolean {
  return isSplitter(kind) || isMerger(kind);
}

function portCount(kind: string): { inputPorts: number; outputPorts: number } {
  if (kind === 'splitter2') return { inputPorts: 1, outputPorts: 2 };
  if (kind === 'splitter3') return { inputPorts: 1, outputPorts: 3 };
  if (kind === 'merger2') return { inputPorts: 2, outputPorts: 1 };
  if (kind === 'merger3') return { inputPorts: 3, outputPorts: 1 };
  return {
    inputPorts: kind === 'input' ? 0 : 1,
    outputPorts: kind === 'input' ? 1 : 0,
  };
}

let endpointTextContext: CanvasRenderingContext2D | null | undefined;

function measureEndpointTextWidth(text: string): number {
  const context = endpointMeasureContext();
  if (context) {
    const width = context.measureText(text).width;
    if (width > 0) return width;
  }
  return text.length * endpointFallbackCharWidth;
}

function endpointMeasureContext(): CanvasRenderingContext2D | null {
  if (endpointTextContext !== undefined) return endpointTextContext;
  endpointTextContext = null;
  if (typeof document === 'undefined') return null;
  if (typeof navigator !== 'undefined' && /jsdom/i.test(navigator.userAgent)) return null;
  const context = document.createElement('canvas').getContext('2d');
  if (!context) return null;
  context.font = endpointFont;
  if (context.measureText('M').width <= 0) return null;
  endpointTextContext = context;
  return context;
}

export function endpointNodeDimensions(label: string): {
  width: number;
  height: number;
} {
  const textWidth = measureEndpointTextWidth(label);
  return {
    width: Math.max(endpointHeight, Math.ceil(textWidth + endpointPaddingX)),
    height: endpointHeight,
  };
}

function nodeDimensions(kind: string, label: string): { width: number; height: number } {
  return isDevice(kind) ? { width: deviceSize, height: deviceSize } : endpointNodeDimensions(endpointLabel(label));
}

function layerConstraint(kind: string): Record<string, string> {
  if (kind === 'input') {
    return { 'elk.layered.layering.layerConstraint': 'FIRST_SEPARATE' };
  }
  if (kind === 'output' || kind === 'discard') {
    return { 'elk.layered.layering.layerConstraint': 'LAST_SEPARATE' };
  }
  return {};
}

function permutations<T>(values: T[], length: number): T[][] {
  if (length === 0) return [[]];
  return values.flatMap((value, index) =>
    permutations(
      values.filter((_, candidate) => candidate !== index),
      length - 1,
    ).map((tail) => [value, ...tail]),
  );
}

export function choosePortSides(
  direction: 'input' | 'output',
  bounds: NodeBounds,
  connections: PortConnection[],
): PortSide[] {
  const primary: PortSide = direction === 'input' ? 'left' : 'right';
  if (connections.length <= 1) return [primary];
  const candidates: PortSide[] = [primary, 'top', 'bottom'];
  const orderedConnections = [...connections].sort((left, right) => left.port - right.port);
  let bestSides = candidates.slice(0, orderedConnections.length);
  let bestScore = Number.POSITIVE_INFINITY;

  for (const sides of permutations(candidates, orderedConnections.length)) {
    const points = sides.map((side) => handlePoint(bounds, side));
    const distance = orderedConnections.reduce((total, connection, index) => {
      const point = points[index];
      return total + Math.abs(point.x - connection.other.x) + Math.abs(point.y - connection.other.y);
    }, 0);
    let crossings = 0;
    for (let left = 0; left < orderedConnections.length; left += 1) {
      for (let right = left + 1; right < orderedConnections.length; right += 1) {
        const otherOrder = orderedConnections[left].other.y - orderedConnections[right].other.y;
        const handleOrder = points[left].y - points[right].y;
        if (otherOrder * handleOrder < 0) crossings += 1;
      }
    }
    const score = distance + crossings * 100_000;
    if (score < bestScore) {
      bestScore = score;
      bestSides = sides;
    }
  }

  const byPort = Array<PortSide>(connections.length).fill(primary);
  orderedConnections.forEach((connection, index) => {
    byPort[connection.port] = bestSides[index];
  });
  return byPort;
}

function endpointSide(
  nodeId: string,
  port: number,
  direction: 'input' | 'output',
  kinds: Map<string, string>,
  assignments: PortAssignments,
): PortSide {
  if (!isDevice(kinds.get(nodeId) ?? '')) return direction === 'input' ? 'left' : 'right';
  const sides = direction === 'input' ? assignments.inputs.get(nodeId) : assignments.outputs.get(nodeId);
  return sides?.[port] ?? (direction === 'input' ? 'left' : 'right');
}

function graphPortScore(
  solution: Solution,
  bounds: Map<string, NodeBounds>,
  kinds: Map<string, string>,
  assignments: PortAssignments,
): number {
  const endpoints = solution.edges.map((edge) => {
    const sourceSide = endpointSide(edge.source, edge.sourcePort, 'output', kinds, assignments);
    const targetSide = endpointSide(edge.target, edge.targetPort, 'input', kinds, assignments);
    const source = handlePoint(bounds.get(edge.source)!, sourceSide);
    const target = handlePoint(bounds.get(edge.target)!, targetSide);
    return { edge, source, sourceSide, target, targetSide };
  });
  let score = endpoints.reduce(
    (total, endpoint) =>
      total + Math.abs(endpoint.source.x - endpoint.target.x) + Math.abs(endpoint.source.y - endpoint.target.y),
    0,
  );

  const parallelCounts = new Map<string, number>();
  for (const { edge } of endpoints) {
    const key = `${edge.source}\0${edge.target}`;
    parallelCounts.set(key, (parallelCounts.get(key) ?? 0) + 1);
  }
  for (const endpoint of endpoints) {
    const key = `${endpoint.edge.source}\0${endpoint.edge.target}`;
    if ((parallelCounts.get(key) ?? 0) < 2) continue;
    const aligned =
      (endpoint.sourceSide === 'right' && endpoint.targetSide === 'left') ||
      (endpoint.sourceSide === 'top' && endpoint.targetSide === 'top') ||
      (endpoint.sourceSide === 'bottom' && endpoint.targetSide === 'bottom');
    if (!aligned) score += 100_000;
  }

  for (const node of solution.nodes) {
    for (const direction of ['input', 'output'] as const) {
      const incident = endpoints.filter(({ edge }) =>
        direction === 'input' ? edge.target === node.id : edge.source === node.id,
      );
      for (let left = 0; left < incident.length; left += 1) {
        for (let right = left + 1; right < incident.length; right += 1) {
          const leftOwn = direction === 'input' ? incident[left].target : incident[left].source;
          const rightOwn = direction === 'input' ? incident[right].target : incident[right].source;
          const leftOther = direction === 'input' ? incident[left].source : incident[left].target;
          const rightOther = direction === 'input' ? incident[right].source : incident[right].target;
          if ((leftOwn.y - rightOwn.y) * (leftOther.y - rightOther.y) < 0) score += 100_000;
        }
      }
    }
  }
  return score;
}

function optimizePortSides(
  solution: Solution,
  bounds: Map<string, NodeBounds>,
  kinds: Map<string, string>,
  assignments: PortAssignments,
): void {
  for (let pass = 0; pass < 8; pass += 1) {
    let changed = false;
    for (const node of solution.nodes) {
      const counts = portCount(node.kind);
      for (const direction of ['input', 'output'] as const) {
        const count = direction === 'input' ? counts.inputPorts : counts.outputPorts;
        if (count <= 1) continue;
        const map = direction === 'input' ? assignments.inputs : assignments.outputs;
        const primary: PortSide = direction === 'input' ? 'left' : 'right';
        const choices = permutations<PortSide>([primary, 'top', 'bottom'], count);
        const current = map.get(node.id) ?? choices[0];
        let best = current;
        let bestScore = graphPortScore(solution, bounds, kinds, assignments);
        for (const choice of choices) {
          map.set(node.id, choice);
          const score = graphPortScore(solution, bounds, kinds, assignments);
          if (score < bestScore) {
            best = choice;
            bestScore = score;
          }
        }
        if (current.some((side, port) => side !== best[port])) changed = true;
        map.set(node.id, best);
      }
    }
    if (!changed) break;
  }
}

function layoutPortId(nodeId: string, direction: 'input' | 'output', port: number): string {
  return `${nodeId}:${direction}:${port}`;
}

function layoutPorts(nodeId: string, inputPositions: PortSide[], outputPositions: PortSide[]) {
  return [
    ...inputPositions.map((side, port) => ({
      id: layoutPortId(nodeId, 'input', port),
      width: 0,
      height: 0,
      layoutOptions: { 'elk.port.side': portSideToElk(side) },
    })),
    ...outputPositions.map((side, port) => ({
      id: layoutPortId(nodeId, 'output', port),
      width: 0,
      height: 0,
      layoutOptions: { 'elk.port.side': portSideToElk(side) },
    })),
  ];
}

export async function layoutSolution(solution: Solution): Promise<FlowGraph> {
  const { default: ELK } = await import('elkjs/lib/elk.bundled.js');
  const elk = new ELK();
  const layoutOptions = {
    'elk.algorithm': 'layered',
    'elk.direction': 'RIGHT',
    'elk.edgeRouting': 'ORTHOGONAL',
    'elk.spacing.nodeNode': '52',
    'elk.layered.spacing.nodeNodeBetweenLayers': '112',
    'elk.layered.cycleBreaking.strategy': 'DEPTH_FIRST',
    'elk.layered.nodePlacement.favorStraightEdges': 'true',
    'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
    'elk.layered.crossingMinimization.greedySwitch.type': 'TWO_SIDED',
    'elk.layered.thoroughness': '30',
  };
  const dimensionsById = new Map(solution.nodes.map((node) => [node.id, nodeDimensions(node.kind, node.label)]));
  const fallbackBox = (nodeId: string): NodeBounds => ({
    x: 0,
    y: 0,
    ...(dimensionsById.get(nodeId) ?? {
      width: endpointHeight,
      height: endpointHeight,
    }),
  });
  const preliminaryGraph = await elk.layout({
    id: 'root',
    layoutOptions,
    children: solution.nodes.map((node) => ({
      id: node.id,
      ...dimensionsById.get(node.id)!,
      layoutOptions: layerConstraint(node.kind),
    })),
    edges: solution.edges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target],
      layoutOptions: {
        'elk.layered.priority.direction': edge.feedback ? '0' : '100',
      },
    })),
  });

  const preliminaryBounds = new Map(
    (preliminaryGraph.children ?? []).map((node) => [
      node.id,
      {
        x: node.x ?? 0,
        y: node.y ?? 0,
        width: node.width ?? dimensionsById.get(node.id)?.width ?? endpointHeight,
        height: node.height ?? dimensionsById.get(node.id)?.height ?? endpointHeight,
      },
    ]),
  );
  const kinds = new Map(solution.nodes.map((node) => [node.id, node.kind]));
  const inputPositions = new Map<string, PortSide[]>();
  const outputPositions = new Map<string, PortSide[]>();
  const center = (nodeId: string) => {
    const node = preliminaryBounds.get(nodeId) ?? fallbackBox(nodeId);
    return { x: node.x + node.width / 2, y: node.y + node.height / 2 };
  };
  for (const node of solution.nodes) {
    const ports = portCount(node.kind);
    const nodeBounds = preliminaryBounds.get(node.id) ?? fallbackBox(node.id);
    const incoming = solution.edges
      .filter((edge) => edge.target === node.id)
      .map((edge) => ({ port: edge.targetPort, other: center(edge.source) }));
    const outgoing = solution.edges
      .filter((edge) => edge.source === node.id)
      .map((edge) => ({ port: edge.sourcePort, other: center(edge.target) }));
    inputPositions.set(node.id, ports.inputPorts > 0 ? choosePortSides('input', nodeBounds, incoming) : []);
    outputPositions.set(node.id, ports.outputPorts > 0 ? choosePortSides('output', nodeBounds, outgoing) : []);
  }
  optimizePortSides(solution, preliminaryBounds, kinds, {
    inputs: inputPositions,
    outputs: outputPositions,
  });

  const graph = await elk.layout({
    id: 'root',
    layoutOptions,
    children: solution.nodes.map((node) => ({
      id: node.id,
      ...dimensionsById.get(node.id)!,
      ports: layoutPorts(node.id, inputPositions.get(node.id) ?? [], outputPositions.get(node.id) ?? []),
      layoutOptions: {
        ...layerConstraint(node.kind),
        'elk.portConstraints': 'FIXED_SIDE',
      },
    })),
    edges: solution.edges.map((edge) => ({
      id: edge.id,
      sources: [layoutPortId(edge.source, 'output', edge.sourcePort)],
      targets: [layoutPortId(edge.target, 'input', edge.targetPort)],
      layoutOptions: {
        'elk.layered.priority.direction': edge.feedback ? '0' : '100',
      },
    })),
  });
  const bounds = new Map(
    (graph.children ?? []).map((node) => [
      node.id,
      {
        x: node.x ?? 0,
        y: node.y ?? 0,
        width: node.width ?? dimensionsById.get(node.id)?.width ?? endpointHeight,
        height: node.height ?? dimensionsById.get(node.id)?.height ?? endpointHeight,
      },
    ]),
  );
  let splitterNumber = 0;
  let mergerNumber = 0;

  const nodes = solution.nodes.map((node) => {
    const dimensions = dimensionsById.get(node.id)!;
    const label = isSplitter(node.kind)
      ? `S${++splitterNumber}`
      : isMerger(node.kind)
        ? `M${++mergerNumber}`
        : endpointLabel(node.label);
    return {
      id: node.id,
      type: isDevice(node.kind) ? 'factory' : node.kind === 'input' ? 'input' : 'output',
      position: snapPositionToGrid(bounds.get(node.id) ?? { x: 0, y: 0 }),
      sourcePosition: Position.Right,
      targetPosition: Position.Left,
      width: dimensions.width,
      height: dimensions.height,
      data: {
        label,
        inputPositions: inputPositions.get(node.id) ?? [],
        outputPositions: outputPositions.get(node.id) ?? [],
      },
      style: `width: ${dimensions.width}px; height: ${dimensions.height}px;`,
      class: nodeClass(node.kind),
    };
  });
  const edges = solution.edges.map((edge): Edge => {
    const loopback = edge.feedback;
    return {
      id: edge.id,
      source: edge.source,
      target: edge.target,
      sourceHandle: isDevice(kinds.get(edge.source) ?? '') ? `source-${edge.sourcePort}` : undefined,
      targetHandle: isDevice(kinds.get(edge.target) ?? '') ? `target-${edge.targetPort}` : undefined,
      type: 'smoothstep',
      label: edge.rate.exact,
      markerEnd: {
        type: MarkerType.ArrowClosed,
        width: 18,
        height: 18,
        strokeWidth: 1.5,
      },
      class: edge.discarded
        ? 'factory-edge factory-edge--discard'
        : loopback
          ? 'factory-edge factory-edge--feedback'
          : 'factory-edge',
      ariaLabel: `${edge.rate.exact} items per minute from ${edge.source} to ${edge.target}${loopback ? ', feedback belt' : ''}${edge.discarded ? ', discard belt' : ''}`,
    };
  });
  return { nodes, edges };
}
