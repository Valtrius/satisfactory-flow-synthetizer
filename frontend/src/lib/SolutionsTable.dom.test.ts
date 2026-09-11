import { flushSync, mount, unmount } from 'svelte';
import { expect, it, vi } from 'vitest';
import SolutionsTable from './SolutionsTable.svelte';
import { solution } from '../test/fixtures';
import { DEFAULT_SORT_COLUMNS } from './solutionSort';

it('provides a keyboard entry point and selects layouts without duplicate activation', async () => {
  const onSelect = vi.fn();
  const component = mount(SolutionsTable, {
    target: document.body,
    props: {
      solutions: [solution, solution],
      selectedIndex: 0,
      columns: [...DEFAULT_SORT_COLUMNS],
      onSelect,
      onColumnsChange: vi.fn(),
    },
  });
  flushSync();
  try {
    const buttons = [...document.querySelectorAll<HTMLButtonElement>('[data-layout-select]')];
    expect(buttons.map((button) => button.tabIndex)).toEqual([0, -1]);
    buttons[0].focus();
    buttons[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    expect(document.activeElement).toBe(buttons[1]);
    expect(onSelect).toHaveBeenLastCalledWith(1);
    buttons[1].dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
    expect(document.activeElement).toBe(buttons[0]);
    onSelect.mockClear();
    buttons[1].click();
    expect(onSelect).toHaveBeenCalledExactlyOnceWith(1);
    expect(document.querySelector('th')!.getAttribute('aria-sort')).toBe('ascending');
  } finally {
    await unmount(component);
  }
});
