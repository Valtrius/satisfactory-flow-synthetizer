import { describe, expect, it } from 'vitest';
import { MarkerType } from '@xyflow/svelte';
import {
  choosePortSides,
  endpointNodeDimensions,
  layoutSolution,
  rotateDevicePorts,
  rotateFlowGraph,
  swapDeviceSides,
} from './graph';
import type { Solution } from '../types';

const solution: Solution = {
  engine: 'z3',
  status: 'proven_optimal',
  modelVersion: 1,
  stats: {
    nodeCount: 1,
    splitters: 1,
    mergers: 0,
    feedbackLoops: 0,
    linkCount: 0,
    checkedThrough: 1,
    beltCount: 0,
    internalMaxThroughput: { exact: '0', decimal: '0' },
  },
  totalInput: { exact: '120', decimal: '120' },
  totalOutput: { exact: '120', decimal: '120' },
  discardRate: { exact: '0', decimal: '0' },
  beltRate: { exact: '1200', decimal: '1200' },
  nodes: [
    { id: 'input-0', kind: 'input', label: 'Input 1 · 120 /min' },
    { id: 'splitter-0', kind: 'splitter2', label: 'Splitter 1' },
    { id: 'output-0', kind: 'output', label: 'Output 1 · 60 /min' },
    { id: 'output-1', kind: 'output', label: 'Output 2 · 60 /min' },
  ],
  edges: [
    {
      id: 'edge-0',
      source: 'input-0',
      target: 'splitter-0',
      sourcePort: 0,
      targetPort: 0,
      rate: { exact: '120', decimal: '120' },
      feedback: false,
      discarded: false,
    },
    {
      id: 'edge-1',
      source: 'splitter-0',
      target: 'output-0',
      sourcePort: 0,
      targetPort: 0,
      rate: { exact: '60', decimal: '60' },
      feedback: false,
      discarded: false,
    },
    {
      id: 'edge-2',
      source: 'splitter-0',
      target: 'output-1',
      sourcePort: 1,
      targetPort: 0,
      rate: { exact: '60', decimal: '60' },
      feedback: false,
      discarded: false,
    },
  ],
  buildSteps: [],
};

describe('endpointNodeDimensions', () => {
  it('is at least as wide as the node is tall', () => {
    const size = endpointNodeDimensions('1');
    expect(size.width).toBe(size.height);
    expect(size.height).toBe(76);
  });

  it('grows to fit the label plus padding', () => {
    const size = endpointNodeDimensions('12345678901234567890');
    expect(size.width).toBeGreaterThan(size.height);
  });
});

describe('layoutSolution', () => {
  it('swaps only the requested physical port sides', () => {
    const inputs = ['left'] as const;
    const outputs = ['right', 'bottom'] as const;
    expect(swapDeviceSides([...inputs], [...outputs], 'left', 'bottom')).toEqual({
      inputPositions: ['bottom'],
      outputPositions: ['right', 'left'],
    });
    expect(inputs).toEqual(['left']);
    expect(outputs).toEqual(['right', 'bottom']);
  });

  it('moves a link onto an empty side', () => {
    expect(swapDeviceSides(['left'], ['right', 'bottom'], 'left', 'top')).toEqual({
      inputPositions: ['top'],
      outputPositions: ['right', 'bottom'],
    });
  });

  it('swaps any pair of sides including empty ones', () => {
    expect(swapDeviceSides(['left'], ['right', 'bottom'], 'top', 'right')).toEqual({
      inputPositions: ['left'],
      outputPositions: ['top', 'bottom'],
    });
  });

  it('switches interchangeable handles to the closest non-crossing sides', () => {
    const bounds = { x: 100, y: 100, width: 76, height: 76 };

    expect(
      choosePortSides('output', bounds, [
        { port: 0, other: { x: 138, y: 300 } },
        { port: 1, other: { x: 300, y: 138 } },
      ]),
    ).toEqual(['bottom', 'right']);
    expect(
      choosePortSides('input', bounds, [
        { port: 0, other: { x: 138, y: 300 } },
        { port: 1, other: { x: 138, y: 0 } },
        { port: 2, other: { x: 0, y: 138 } },
      ]),
    ).toEqual(['bottom', 'top', 'left']);
  });

  it('lays out every verified node and keeps exact belt labels', async () => {
    const graph = await layoutSolution(solution);

    expect(graph.nodes).toHaveLength(solution.nodes.length);
    expect(graph.edges).toHaveLength(solution.edges.length);
    expect(graph.nodes.every((node) => Number.isFinite(node.position.x))).toBe(true);
    expect(graph.nodes[0].data.label).toBe('120');
    expect(graph.nodes[1].data.label).toBe('S1');
    expect(graph.nodes[1].type).toBe('factory');
    expect(graph.edges[1].label).toBe('60');
    expect(graph.edges[1].sourceHandle).toBe('source-0');
    expect(graph.edges[2].sourceHandle).toBe('source-1');

    const input = graph.nodes.find((node) => node.id === 'input-0');
    const splitter = graph.nodes.find((node) => node.id === 'splitter-0');
    const outputs = graph.nodes.filter((node) => node.id.startsWith('output-'));
    const inputSize = endpointNodeDimensions('120');
    const outputSize = endpointNodeDimensions('60');
    expect(input).toMatchObject({
      width: inputSize.width,
      height: inputSize.height,
      style: `width: ${inputSize.width}px; height: ${inputSize.height}px;`,
    });
    expect(outputs.every((node) => node.width === outputSize.width)).toBe(true);
    expect(outputs.every((node) => node.height === outputSize.height)).toBe(true);
    expect(input!.position.x).toBeLessThan(splitter!.position.x);
    expect(outputs.every((node) => node.position.x === outputs[0].position.x)).toBe(true);
    expect(outputs[0].position.x).toBeGreaterThan(splitter!.position.x);
    expect(graph.edges.every((edge) => edge.type === 'smoothstep')).toBe(true);
    expect(graph.edges.every((edge) => edge.data === undefined)).toBe(true);
    expect(
      graph.edges.every(
        (edge) =>
          typeof edge.markerEnd === 'object' &&
          edge.markerEnd !== null &&
          'type' in edge.markerEnd &&
          edge.markerEnd.type === MarkerType.ArrowClosed,
      ),
    ).toBe(true);
    expect(graph.edges[0].class).toBe('factory-edge');
  });

  it('uses short merger names and separate physical input handles', async () => {
    const mergerSolution: Solution = {
      ...solution,
      stats: {
        ...solution.stats,
        splitters: 0,
        mergers: 1,
      },
      nodes: [
        { id: 'input-0', kind: 'input', label: 'Input 1' },
        { id: 'input-1', kind: 'input', label: 'Input 2' },
        { id: 'merger-0', kind: 'merger2', label: '2-way merger 1' },
        { id: 'output-0', kind: 'output', label: 'Output 1' },
      ],
      edges: [
        {
          ...solution.edges[0],
          id: 'edge-0',
          source: 'input-0',
          target: 'merger-0',
        },
        {
          ...solution.edges[0],
          id: 'edge-1',
          source: 'input-1',
          target: 'merger-0',
          targetPort: 1,
        },
        {
          ...solution.edges[0],
          id: 'edge-2',
          source: 'merger-0',
          target: 'output-0',
        },
      ],
    };

    const graph = await layoutSolution(mergerSolution);
    const merger = graph.nodes.find((node) => node.id === 'merger-0');
    const inputs = graph.nodes.filter((node) => node.id.startsWith('input-'));

    expect(merger!.data.label).toBe('M1');
    expect(graph.edges[0].targetHandle).toBe('target-0');
    expect(graph.edges[1].targetHandle).toBe('target-1');
    expect(inputs.every((node) => node.position.x === inputs[0].position.x)).toBe(true);
    expect(inputs[0].position.x).toBeLessThan(merger!.position.x);
  });

  it('pairs parallel splitter and merger handles without crossed routes', async () => {
    const parallelSolution: Solution = {
      ...solution,
      nodes: [
        { id: 'input-0', kind: 'input', label: 'Input' },
        { id: 'splitter-0', kind: 'splitter3', label: 'Splitter' },
        { id: 'merger-0', kind: 'merger3', label: 'Merger' },
        { id: 'output-0', kind: 'output', label: 'Output' },
      ],
      edges: [
        {
          ...solution.edges[0],
          id: 'edge-in',
          source: 'input-0',
          target: 'splitter-0',
        },
        ...[2, 0, 1].map((targetPort, sourcePort) => ({
          ...solution.edges[0],
          id: `edge-parallel-${sourcePort}`,
          source: 'splitter-0',
          target: 'merger-0',
          sourcePort,
          targetPort,
        })),
        {
          ...solution.edges[0],
          id: 'edge-out',
          source: 'merger-0',
          target: 'output-0',
        },
      ],
    };

    const graph = await layoutSolution(parallelSolution);
    const splitter = graph.nodes.find((node) => node.id === 'splitter-0')!;
    const merger = graph.nodes.find((node) => node.id === 'merger-0')!;
    const sourceSides = (splitter.data as { outputPositions: string[] }).outputPositions;
    const targetSides = (merger.data as { inputPositions: string[] }).inputPositions;
    const alignedPairs = new Set(['right:left', 'top:top', 'bottom:bottom']);

    for (const edge of parallelSolution.edges.filter((edge) => edge.id.startsWith('edge-parallel'))) {
      expect(alignedPairs.has(`${sourceSides[edge.sourcePort]}:${targetSides[edge.targetPort]}`)).toBe(true);
    }
  });

  it('aligns the longest direct splitter chain with horizontal item flow', async () => {
    const chainSolution: Solution = {
      ...solution,
      nodes: [
        { id: 'input-0', kind: 'input', label: 'Input' },
        { id: 'splitter-0', kind: 'splitter2', label: 'Splitter 1' },
        { id: 'splitter-1', kind: 'splitter2', label: 'Splitter 2' },
        { id: 'splitter-2', kind: 'splitter2', label: 'Splitter 3' },
        { id: 'splitter-3', kind: 'splitter2', label: 'Splitter 4' },
        { id: 'output-0', kind: 'output', label: 'Output 1' },
        { id: 'output-1', kind: 'output', label: 'Output 2' },
      ],
      edges: [
        {
          ...solution.edges[0],
          id: 'edge-in',
          source: 'input-0',
          target: 'splitter-0',
        },
        {
          ...solution.edges[0],
          id: 'edge-short',
          source: 'splitter-0',
          target: 'splitter-1',
        },
        {
          ...solution.edges[0],
          id: 'edge-chain-0',
          source: 'splitter-0',
          target: 'splitter-2',
          sourcePort: 1,
        },
        {
          ...solution.edges[0],
          id: 'edge-chain-1',
          source: 'splitter-2',
          target: 'splitter-3',
        },
        {
          ...solution.edges[0],
          id: 'edge-short-out',
          source: 'splitter-1',
          target: 'output-0',
        },
        {
          ...solution.edges[0],
          id: 'edge-chain-out',
          source: 'splitter-3',
          target: 'output-1',
        },
      ],
    };

    const graph = await layoutSolution(chainSolution);
    const centers = ['splitter-0', 'splitter-2', 'splitter-3'].map((nodeId) => {
      const node = graph.nodes.find((candidate) => candidate.id === nodeId)!;
      return node.position.y + 38;
    });
    expect(new Set(centers).size).toBe(1);
  });

  it('rotates every occupied port 90 degrees', () => {
    expect(rotateDevicePorts(['left'], ['right', 'bottom'], 'cw')).toEqual({
      inputPositions: ['top'],
      outputPositions: ['bottom', 'left'],
    });
  });

  it('rotates the graph 90 degrees while keeping relative node positions', async () => {
    const graph = await layoutSolution(solution);
    const inputBefore = graph.nodes.find((node) => node.id === 'input-0')!;
    const outputBefore = graph.nodes.find((node) => node.id === 'output-0')!;
    const dx = outputBefore.position.x - inputBefore.position.x;
    const dy = outputBefore.position.y - inputBefore.position.y;
    const rotated = rotateFlowGraph(graph.nodes, graph.edges, 'cw');
    const inputAfter = rotated.nodes.find((node) => node.id === 'input-0')!;
    const outputAfter = rotated.nodes.find((node) => node.id === 'output-0')!;
    expect(outputAfter.position.x - inputAfter.position.x).toBeCloseTo(-dy);
    expect(outputAfter.position.y - inputAfter.position.y).toBeCloseTo(dx);
    expect(rotated.nodes.find((node) => node.id === 'splitter-0')!.data.outputPositions).not.toEqual(
      graph.nodes.find((node) => node.id === 'splitter-0')!.data.outputPositions,
    );
    const restored = rotateFlowGraph(rotated.nodes, rotated.edges, 'ccw');
    for (const node of restored.nodes) {
      const original = graph.nodes.find((candidate) => candidate.id === node.id)!;
      expect(node.position.x).toBeCloseTo(original.position.x);
      expect(node.position.y).toBeCloseTo(original.position.y);
    }
  });
});
