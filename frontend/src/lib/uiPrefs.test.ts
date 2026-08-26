import { describe, expect, it } from 'vitest';
import {
  DEFAULT_FORM_DRAFT_PREFS,
  DEFAULT_HISTORY_TOOLBAR_PREFS,
  parseFormDraftPrefs,
  parseHistoryToolbarPrefs,
  parseUiPrefs
} from './uiPrefs';

describe('parseHistoryToolbarPrefs', () => {
  it('keeps valid sort and filter chips', () => {
    expect(
      parseHistoryToolbarPrefs({
        query: '60',
        sort: 'newest',
        statusFilters: ['completed', 'bogus', 'failed', 'completed'],
        engineFilters: ['z3'],
        searchFilters: ['all']
      })
    ).toEqual({
      query: '60',
      sort: 'newest',
      statusFilters: ['completed', 'failed'],
      engineFilters: ['z3'],
      searchFilters: ['all']
    });
  });

  it('falls back for unknown shapes', () => {
    expect(parseHistoryToolbarPrefs(null)).toEqual(DEFAULT_HISTORY_TOOLBAR_PREFS);
    expect(parseHistoryToolbarPrefs({ sort: 'nope' }).sort).toBe('manual');
  });
});

describe('parseFormDraftPrefs', () => {
  it('restores a form draft', () => {
    expect(
      parseFormDraftPrefs({
        inputs: [{ id: 'input-1', name: '', rate: '120', multiplier: '2' }],
        outputs: [{ id: 'output-1', name: '', rate: '40', multiplier: '1' }],
        beltRate: '780',
        enumerateAllAtN: false,
        engine: 'z3',
        nextEndpointId: 9
      })
    ).toEqual({
      inputs: [{ id: 'input-1', name: '', rate: '120', multiplier: '2' }],
      outputs: [{ id: 'output-1', name: '', rate: '40', multiplier: '1' }],
      beltRate: '780',
      enumerateAllAtN: false,
      engine: 'z3',
      nextEndpointId: 9
    });
  });

  it('falls back for invalid drafts', () => {
    const parsed = parseFormDraftPrefs({ inputs: 'nope', engine: 'mystery' });
    expect(parsed.inputs).toEqual([]);
    expect(parsed.engine).toBe('custom');
    expect(parsed.outputs).toEqual(DEFAULT_FORM_DRAFT_PREFS.outputs);
  });
});

describe('parseUiPrefs', () => {
  it('parses a full document', () => {
    const parsed = parseUiPrefs({
      version: 1,
      history: { query: 'x', sort: 'oldest' },
      form: { beltRate: '600', engine: 'z3', enumerateAllAtN: false }
    });
    expect(parsed.history.query).toBe('x');
    expect(parsed.history.sort).toBe('oldest');
    expect(parsed.form.beltRate).toBe('600');
    expect(parsed.form.engine).toBe('z3');
  });
});
