import { describe, expect, it } from 'vitest';
import {
  MAX_ENDPOINTS,
  buildSolveRequest,
  clampMultiplier,
  createEndpointRow,
  endpointSlots,
  expandEndpoints,
  parseMultiplier,
} from './endpoints';

describe('endpoints', () => {
  it('parses multipliers with a floor of 1', () => {
    expect(parseMultiplier('3')).toBe(3);
    expect(parseMultiplier(' 2 ')).toBe(2);
    expect(parseMultiplier('0')).toBe(1);
    expect(parseMultiplier('')).toBe(1);
    expect(parseMultiplier('x')).toBe(1);
  });

  it('clamps multipliers to the endpoint cap', () => {
    expect(clampMultiplier('99')).toBe(String(MAX_ENDPOINTS));
    expect(clampMultiplier('2')).toBe('2');
  });

  it('counts expanded slots', () => {
    expect(
      endpointSlots([
        { id: 'a', name: '', rate: '60', multiplier: '2' },
        { id: 'b', name: '', rate: '60', multiplier: '3' },
      ]),
    ).toBe(5);
  });

  it('creates default rows', () => {
    expect(createEndpointRow('inputs', 4)).toEqual({
      id: 'input-4',
      name: '',
      rate: '60',
      multiplier: '1',
    });
    expect(createEndpointRow('outputs', 1).id).toBe('output-1');
  });

  it('expands rows and respects the global cap', () => {
    const expanded = expandEndpoints([
      { id: 'in-1', name: 'Iron', rate: '60', multiplier: '2' },
      { id: 'in-2', name: '', rate: '120', multiplier: '1' },
    ]);
    expect(expanded).toEqual([
      { id: 'in-1-1', name: 'Iron', rate: '60' },
      { id: 'in-1-2', name: 'Iron', rate: '60' },
      { id: 'in-2', name: '', rate: '120' },
    ]);

    const capped = expandEndpoints([{ id: 'big', name: '', rate: '60', multiplier: '100' }], 3);
    expect(capped).toHaveLength(3);
    expect(capped[0]?.id).toBe('big-1');
  });

  it('serializes the selected engine into the solve request', () => {
    const custom = buildSolveRequest([], [{ id: 'o', name: '', rate: '60', multiplier: '1' }], '1200', true);
    expect(custom.engine).toBe('custom');
    const z3 = buildSolveRequest([], [{ id: 'o', name: '', rate: '60', multiplier: '1' }], '1200', false, 'z3');
    expect(z3.engine).toBe('z3');
    expect(z3.enumerateAllAtN).toBe(false);
  });
});
