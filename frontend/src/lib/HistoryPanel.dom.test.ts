import { mount, unmount, flushSync } from 'svelte';
import { expect, it, vi } from 'vitest';
import HistoryPanel from './HistoryPanel.svelte';
import { historyEntry } from '../test/fixtures';

function setup() {
  localStorage.clear();
  const onRename = vi.fn(),
    onSelect = vi.fn();
  const component = mount(HistoryPanel, {
    target: document.body,
    props: {
      queued: [],
      running: null,
      history: [historyEntry()],
      selectedEntryId: null,
      runningElapsedLabel: '',
      onSelect,
      onRename,
      onReorderQueued: vi.fn(),
      onReorderHistory: vi.fn(),
      onDelete: vi.fn(),
      onCancelRunning: vi.fn(),
      onCopyToNew: vi.fn(),
      onExportEntry: vi.fn(),
      onExportAll: vi.fn(),
      onImport: vi.fn(),
      onDeleteAll: vi.fn(),
    },
  });
  flushSync();
  return { component, onRename, onSelect };
}

function renameInput(): HTMLInputElement {
  document.querySelector<HTMLButtonElement>('[aria-label="More actions"]')!.click();
  flushSync();
  Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'))
    .find((item) => item.textContent?.includes('Rename'))!
    .click();
  flushSync();
  return document.querySelector<HTMLInputElement>('[aria-label="Rename history entry"]')!;
}

it('lets spaces reach the rename input and commits a multiword title without selecting the row', async () => {
  const { component, onRename, onSelect } = setup();
  try {
    const input = renameInput();
    expect(document.activeElement).toBe(input);
    const space = new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true });
    input.dispatchEvent(space);
    expect(space.defaultPrevented).toBe(false);
    input.value = 'My saved layout';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    flushSync();
    expect(onRename).toHaveBeenCalledExactlyOnceWith('entry', 'My saved layout');
    expect(onSelect).not.toHaveBeenCalled();
  } finally {
    await unmount(component);
  }
});

it('Escape abandons the rename without committing on blur', async () => {
  const { component, onRename } = setup();
  try {
    const input = renameInput();
    input.value = 'Discard this';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    flushSync();
    expect(document.querySelector('[aria-label="Rename history entry"]')).toBeNull();
    expect(onRename).not.toHaveBeenCalled();
  } finally {
    await unmount(component);
  }
});
