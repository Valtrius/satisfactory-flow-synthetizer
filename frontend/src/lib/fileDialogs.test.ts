import { afterEach, expect, it, vi } from 'vitest';
import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { importHistoryPayload, exportHistoryBundle } from './historyIo';
import { saveSvgFile } from './exportSvg';
import { historyEntry } from '../test/fixtures';

vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock('@tauri-apps/plugin-fs', () => ({ readTextFile: vi.fn(), writeTextFile: vi.fn() }));
afterEach(() => vi.resetAllMocks());

it('reads only the path granted by the import dialog and does nothing after cancellation', async () => {
  vi.mocked(open).mockResolvedValueOnce('D:/Shared/chosen.json').mockResolvedValueOnce(null);
  vi.mocked(readTextFile).mockResolvedValue('{"kind":"history-bundle","entries":[]}');
  expect(await importHistoryPayload()).toEqual({ kind: 'history-bundle', entries: [] });
  expect(readTextFile).toHaveBeenCalledExactlyOnceWith('D:/Shared/chosen.json');
  expect(await importHistoryPayload()).toBeNull();
  expect(readTextFile).toHaveBeenCalledOnce();
});
it('writes history and SVG only to paths granted by their save dialogs', async () => {
  vi.mocked(save)
    .mockResolvedValueOnce('D:/Shared/history.json')
    .mockResolvedValueOnce('D:/Shared/layout.svg')
    .mockResolvedValueOnce(null);
  vi.mocked(writeTextFile).mockResolvedValue(undefined);
  await exportHistoryBundle([historyEntry()]);
  expect(writeTextFile).toHaveBeenLastCalledWith('D:/Shared/history.json', expect.stringContaining('history-bundle'));
  await saveSvgFile('<svg/>', 'layout.svg');
  expect(writeTextFile).toHaveBeenLastCalledWith('D:/Shared/layout.svg', '<svg/>');
  await exportHistoryBundle([]);
  expect(writeTextFile).toHaveBeenCalledTimes(2);
});
it('propagates denied access without reading an alternate location', async () => {
  vi.mocked(open).mockResolvedValue('D:/Shared/chosen.json');
  vi.mocked(readTextFile).mockRejectedValue(new Error('permission denied'));
  await expect(importHistoryPayload()).rejects.toThrow('permission denied');
  expect(readTextFile).toHaveBeenCalledOnce();
});
