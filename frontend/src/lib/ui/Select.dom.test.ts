import { flushSync, mount, unmount } from 'svelte';
import { expect, it } from 'vitest';
import Select from './Select.svelte';

it('keeps the current selection on Escape and commits the keyboard choice on Enter', async () => {
  const component = mount(Select, {
    target: document.body,
    props: {
      label: 'Workers',
      value: 'auto',
      options: [
        { value: 'auto', label: 'Automatic' },
        { value: 4, label: '4 workers' },
        { value: 8, label: '8 workers' },
      ],
    },
  });
  flushSync();
  const trigger = document.querySelector<HTMLButtonElement>('[role="combobox"]')!;
  const press = (key: string) => {
    trigger.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
    flushSync();
  };
  try {
    trigger.focus();
    press('ArrowDown');
    press('End');
    expect(trigger.textContent).toContain('Automatic');
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    press('Escape');
    expect(trigger.textContent).toContain('Automatic');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    press('ArrowDown');
    press('ArrowDown');
    press('Enter');
    expect(trigger.textContent).toContain('4 workers');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(document.activeElement).toBe(trigger);
  } finally {
    await unmount(component);
  }
});

it('dismisses on outside focus without changing the selected worker count', async () => {
  const component = mount(Select, {
    target: document.body,
    props: {
      label: 'Workers',
      value: 4,
      options: [
        { value: 4, label: '4 workers' },
        { value: 8, label: '8 workers' },
      ],
    },
  });
  const outside = document.createElement('button');
  document.body.append(outside);
  flushSync();
  try {
    const trigger = document.querySelector<HTMLButtonElement>('[role="combobox"]')!;
    trigger.focus();
    trigger.click();
    flushSync();
    outside.focus();
    flushSync();
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(trigger.textContent).toContain('4 workers');
  } finally {
    outside.remove();
    await unmount(component);
  }
});
