import { isTauri } from '@tauri-apps/api/core';
import { exportBundlePayload, exportEntryPayload, type HistoryEntry, type HistoryExportPayload } from './historyModel';

function downloadJson(contents: string, fileName: string): void {
  const blob = new Blob([contents], { type: 'application/json;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = fileName;
  link.click();
  URL.revokeObjectURL(url);
}

export async function exportHistoryEntry(entry: HistoryEntry): Promise<void> {
  const payload = exportEntryPayload(entry);
  const fileName = `balancer-history-entry.json`;
  await saveJson(payload, fileName);
}

export async function exportHistoryBundle(entries: HistoryEntry[]): Promise<void> {
  const payload = exportBundlePayload(entries);
  await saveJson(payload, 'balancer-history.json');
}

async function saveJson(payload: HistoryExportPayload, fileName: string): Promise<void> {
  const contents = JSON.stringify(payload, null, 2);
  if (isTauri()) {
    const { save } = await import('@tauri-apps/plugin-dialog');
    const { writeTextFile } = await import('@tauri-apps/plugin-fs');
    const path = await save({
      defaultPath: fileName,
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (!path) return;
    await writeTextFile(path, contents);
    return;
  }
  downloadJson(contents, fileName);
}

export async function importHistoryPayload(): Promise<unknown | null> {
  if (isTauri()) {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const { readTextFile } = await import('@tauri-apps/plugin-fs');
    const path = await open({
      multiple: false,
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (!path || Array.isArray(path)) return null;
    return JSON.parse(await readTextFile(path));
  }
  return readJsonFromFileInput();
}

function readJsonFromFileInput(): Promise<unknown | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) {
        resolve(null);
        return;
      }
      const reader = new FileReader();
      reader.onload = () => {
        try {
          resolve(JSON.parse(String(reader.result)));
        } catch {
          resolve(null);
        }
      };
      reader.onerror = () => resolve(null);
      reader.readAsText(file);
    };
    input.click();
  });
}
