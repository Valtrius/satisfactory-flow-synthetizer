import { expect, it } from 'vitest';
import { Position, type Node } from '@xyflow/svelte';
import { solution } from '../../test/fixtures';
import { applyShareLayout, captureShareLayout, readShareLayout } from './layout';

const original = {
  ...solution,
  nodes: solution.nodes.map((node, index) => ({
    ...node,
    id: ['input-0', 'custom-operator', 'output-0', 'output-1'][index],
  })),
};
const verified = {
  ...original,
  nodes: original.nodes.map((node) => ({ ...node, id: node.id === 'custom-operator' ? 'operator-0' : node.id })),
};
const nodes: Node[] = original.nodes.map((node, index) => ({
  id: node.id,
  position: { x: index * 24 + 7, y: 37 - index * 72 },
  sourcePosition: Position.Bottom,
  targetPosition: Position.Top,
  data: {
    label: 'Ignored label',
    onRotatePorts: () => {},
    inputPositions: index === 0 ? [] : ['top'],
    outputPositions: index === 0 ? ['bottom'] : index === 1 ? ['left', 'bottom'] : [],
  },
}));

it('preserves moved nodes and rotated ports across normalized IDs without copying graph content or callbacks', () => {
  const layout = readShareLayout(JSON.parse(JSON.stringify(captureShareLayout(original, nodes))), verified);
  expect(layout.nodes[1].id).toBe('operator-0');
  const restored = applyShareLayout(
    {
      nodes: verified.nodes.map((node) => ({
        id: node.id,
        position: { x: 0, y: 0 },
        data: { label: 'Verified label' },
      })),
      edges: [],
    },
    layout,
  );
  expect(restored.nodes.map((node) => node.position)).toEqual(nodes.map((node) => node.position));
  expect(restored.nodes[1].data).toEqual({
    label: 'Verified label',
    inputPositions: ['top'],
    outputPositions: ['left', 'bottom'],
  });
  expect(restored.nodes[0].sourcePosition).toBe(Position.Bottom);
  expect(JSON.stringify(layout)).not.toContain('Ignored label');
  expect(JSON.stringify(layout)).not.toContain('onRotatePorts');
});

it.each(['coordinate', 'duplicate', 'unknown', 'ports', 'missing', 'version'])(
  'rejects invalid %s layout data',
  (defect) => {
    const layout = captureShareLayout(original, nodes);
    if (defect === 'coordinate') layout.nodes[0].x = Infinity;
    if (defect === 'duplicate') layout.nodes[1].id = layout.nodes[0].id;
    if (defect === 'unknown') layout.nodes[0].id = 'unverified-node';
    if (defect === 'ports') layout.nodes[1].outputPositions = ['top', 'top'];
    if (defect === 'missing') layout.nodes.pop();
    if (defect === 'version') Object.assign(layout, { version: 2 });
    expect(() => readShareLayout(layout, verified)).toThrow('Invalid graph positions');
  },
);
