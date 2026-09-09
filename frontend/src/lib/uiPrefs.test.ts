import { beforeEach, describe, expect, it } from 'vitest';
import {
  DEFAULT_FORM_DRAFT_PREFS,
  DEFAULT_HISTORY_TOOLBAR_PREFS,
  parseFormDraftPrefs,
  parseHistoryToolbarPrefs,
  parseUiPrefs,
  readUiPrefs,
  resetUiPrefsCache,
  UI_PREFS_STORAGE_KEY,
  writeUiPrefs,
} from './uiPrefs';

beforeEach(() => {
  localStorage.clear();
  resetUiPrefsCache();
});

describe('parseHistoryToolbarPrefs', () => {
  it('keeps valid sort and filter chips', () => {
    expect(
      parseHistoryToolbarPrefs({
        query: '60',
        sort: 'newest',
        statusFilters: ['completed', 'bogus', 'failed', 'completed'],
        searchFilters: ['all_min_nl'],
      }),
    ).toEqual({
      query: '60',
      sort: 'newest',
      statusFilters: ['completed', 'failed'],
      searchFilters: ['all_min_nl'],
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
        solveMode: 'one_min_nl',

        nextEndpointId: 9,
      }),
    ).toEqual({
      inputs: [{ id: 'input-1', name: '', rate: '120', multiplier: '2' }],
      outputs: [{ id: 'output-1', name: '', rate: '40', multiplier: '1' }],
      beltRate: '780',
      solveMode: 'one_min_nl',

      nextEndpointId: 9,
    });
  });

  it('falls back for invalid drafts', () => {
    const parsed = parseFormDraftPrefs({ inputs: 'nope', engine: 'mystery' });
    expect(parsed.inputs).toEqual([]);
    expect(parsed.outputs).toEqual(DEFAULT_FORM_DRAFT_PREFS.outputs);
  });

  it('restores minimum-link enumeration', () => {
    const parsed = parseFormDraftPrefs({
      solveMode: 'all_min_nl',
    });
    expect(parsed.solveMode).toBe('all_min_nl');
  });
});

describe('parseUiPrefs', () => {
  it('parses a full document', () => {
    const parsed = parseUiPrefs({
      version: 2,
      history: { query: 'x', sort: 'oldest' },
      form: { beltRate: '600', solveMode: 'one_min_nl' },
    });
    expect(parsed.history.query).toBe('x');
    expect(parsed.history.sort).toBe('oldest');
    expect(parsed.form.beltRate).toBe('600');
  });

  it('migrates version 1 preferences and removes the version 1 storage entry', () => {
    localStorage.setItem(
      'sfs.ui-prefs.v1',
      JSON.stringify({
        version: 1,
        history: { searchFilters: ['opt', 'all'] },
        form: { enumerateAllAtN: false, beltRate: '600' },
      }),
    );

    const parsed = readUiPrefs();

    expect(parsed.version).toBe(2);
    expect(parsed.form.solveMode).toBe('one_min_nl');
    expect(parsed.history.searchFilters).toEqual(['one_min_nl', 'all_min_n']);
    expect(JSON.parse(localStorage.getItem(UI_PREFS_STORAGE_KEY) ?? '{}')).not.toHaveProperty('form.enumerateAllAtN');
    expect(localStorage.getItem('sfs.ui-prefs.v1')).toBeNull();
  });
});

it('drops solver-type preferences', () => {
  expect(parseFormDraftPrefs({ engine: 'saved-engine' })).not.toHaveProperty('engine');
  expect(parseHistoryToolbarPrefs({ engineFilters: ['saved-engine'] })).not.toHaveProperty('engineFilters');
});

it('does not write solver-type fields back to storage', () => {
  const current = readUiPrefs();
  writeUiPrefs({
    ...current,
    form: Object.assign(current.form, { engine: 'saved-engine' }),
    history: Object.assign(current.history, { engineFilters: ['saved-engine'] }),
  });
  const saved = JSON.parse(localStorage.getItem(UI_PREFS_STORAGE_KEY) ?? '{}');
  expect(saved.form).not.toHaveProperty('engine');
  expect(saved.history).not.toHaveProperty('engineFilters');
});
