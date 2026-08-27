import { describe, expect, it } from 'vitest';
import type { Edge, Node } from '@xyflow/svelte';
import { captureGraphSnapshot, pushGraphUndo, snapshotsEqual } from './graphEditHistory';

function node(id: string, x: number, y: number): Node {
  return {
    id,
    position: { x, y },
    data: { inputPositions: ['left'], outputPositions: ['right'] },
    type: 'factory',
  };
}

const edges: Edge[] = [{ id: 'e1', source: 'a', target: 'b' }];

describe('graphEditHistory', () => {
  it('captures without callbacks and detects position changes', () => {
    const nodes = [
      {
        ...node('a', 0, 0),
        data: {
          inputPositions: ['left'],
          outputPositions: ['right'],
          onSwapSides: () => {},
          onRotatePorts: () => {},
        },
      },
    ];
    const snap = captureGraphSnapshot(nodes, edges);
    expect(snap.nodes[0].data.onSwapSides).toBeUndefined();
    expect(snapshotsEqual(snap, captureGraphSnapshot([node('a', 0, 0)], edges))).toBe(true);
    expect(snapshotsEqual(snap, captureGraphSnapshot([node('a', 10, 0)], edges))).toBe(false);
  });

  it('caps undo stack length', () => {
    let stack: ReturnType<typeof captureGraphSnapshot>[] = [];
    for (let i = 0; i < 60; i += 1) {
      stack = pushGraphUndo(stack, captureGraphSnapshot([node('a', i, 0)], edges));
    }
    expect(stack).toHaveLength(50);
    expect(stack[0].nodes[0].position.x).toBe(10);
  });
});
