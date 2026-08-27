import { describe, expect, it } from 'vitest';
import { Position, type Edge, type Node } from '@xyflow/svelte';
import { defaultSvgFileName, escapeXml, filenameRate, graphToSvg } from './exportSvg';
import { layoutSolution } from './graph';
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

function parseViewBox(svg: string): {
  x: number;
  y: number;
  width: number;
  height: number;
} {
  const match = svg.match(/viewBox="([^"]+)"/);
  expect(match).not.toBeNull();
  const [x, y, width, height] = match![1].split(' ').map(Number);
  return { x, y, width, height };
}

describe('graphToSvg', () => {
  it('fits every node in the viewBox and keeps labels and rates', async () => {
    const graph = await layoutSolution(solution);
    const svg = graphToSvg(graph.nodes, graph.edges);
    const viewBox = parseViewBox(svg);

    for (const node of graph.nodes) {
      const width = node.width ?? 0;
      const height = node.height ?? 0;
      expect(viewBox.x).toBeLessThanOrEqual(node.position.x);
      expect(viewBox.y).toBeLessThanOrEqual(node.position.y);
      expect(viewBox.x + viewBox.width).toBeGreaterThanOrEqual(node.position.x + width);
      expect(viewBox.y + viewBox.height).toBeGreaterThanOrEqual(node.position.y + height);
    }

    expect(svg).toContain('>120<');
    expect(svg).toContain('>S1<');
    expect(svg).toContain('>60<');
  });

  it('styles feedback belts with a dashed warning stroke', () => {
    const nodes: Node[] = [
      {
        id: 'a',
        position: { x: 0, y: 0 },
        width: 76,
        height: 76,
        sourcePosition: Position.Right,
        targetPosition: Position.Left,
        data: {
          label: 'S1',
          inputPositions: ['left'],
          outputPositions: ['right'],
        },
        class: 'factory-node factory-node--splitter2',
      },
      {
        id: 'b',
        position: { x: 200, y: 0 },
        width: 76,
        height: 76,
        sourcePosition: Position.Right,
        targetPosition: Position.Left,
        data: {
          label: 'M1',
          inputPositions: ['left'],
          outputPositions: ['right'],
        },
        class: 'factory-node factory-node--merger2',
      },
    ];
    const edges: Edge[] = [
      {
        id: 'loop',
        source: 'b',
        target: 'a',
        sourceHandle: 'source-0',
        targetHandle: 'target-0',
        label: '30',
        class: 'factory-edge factory-edge--feedback',
      },
    ];

    const svg = graphToSvg(nodes, edges);
    expect(svg).toContain('stroke="#e8b654"');
    expect(svg).toContain('stroke-dasharray="7 5"');
  });

  it('escapes XML in node and edge labels', () => {
    const nodes: Node[] = [
      {
        id: 'a',
        position: { x: 0, y: 0 },
        width: 76,
        height: 76,
        sourcePosition: Position.Right,
        targetPosition: Position.Left,
        data: {
          label: 'A & B < 1',
          outputPositions: ['right'],
          inputPositions: [],
        },
        class: 'factory-node factory-node--input',
      },
      {
        id: 'b',
        position: { x: 160, y: 0 },
        width: 76,
        height: 76,
        sourcePosition: Position.Right,
        targetPosition: Position.Left,
        data: { label: 'out', inputPositions: ['left'], outputPositions: [] },
        class: 'factory-node factory-node--output',
      },
    ];
    const edges: Edge[] = [
      {
        id: 'rate',
        source: 'a',
        target: 'b',
        label: '1 < 2 & 3',
        class: 'factory-edge',
      },
    ];

    const svg = graphToSvg(nodes, edges);
    expect(svg).toContain(escapeXml('A & B < 1'));
    expect(svg).toContain(escapeXml('1 < 2 & 3'));
    expect(svg).not.toContain('A & B < 1');
    expect(svg).not.toContain('1 < 2 & 3');
  });
});

describe('defaultSvgFileName', () => {
  it('encodes grouped rates, multipliers, and node count', () => {
    expect(
      defaultSvgFileName(
        [
          { id: 'in-1', name: '', rate: '10', multiplier: '2' },
          { id: 'in-2', name: '', rate: '30', multiplier: '1' },
        ],
        [{ id: 'out-1', name: '', rate: '12.5', multiplier: '4' }],
        9,
      ),
    ).toBe('2x10-30_to_4x12.5_n-9.svg');
  });

  it('writes terminating fractions as decimals and repeating ones as p-tokens', () => {
    expect(filenameRate('25/2')).toBe('12.5');
    expect(filenameRate('1/8')).toBe('0.125');
    expect(filenameRate('1/3')).toBe('1p3');
    expect(filenameRate('10.0')).toBe('10');
    expect(
      defaultSvgFileName(
        [{ id: 'in-1', name: '', rate: '1/3', multiplier: '3' }],
        [{ id: 'out-1', name: '', rate: '1/3', multiplier: '3' }],
        2,
      ),
    ).toBe('3x1p3_to_3x1p3_n-2.svg');
  });
});
