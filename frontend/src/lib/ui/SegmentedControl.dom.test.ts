import { mount, unmount, flushSync } from 'svelte';
import { expect, it, vi } from 'vitest';
import SegmentedControl from './SegmentedControl.svelte';

it('uses one tab stop and wraps arrow-key selection with focus', async () => {
  const onchange = vi.fn();
  const component = mount(SegmentedControl, {
    target: document.body,
    props: {
      options: [
        { value: 'one', label: 'One' },
        { value: 'all', label: 'All' },
        { value: 'min', label: 'Minimum' },
      ],
      value: 'one',
      onchange,
      'aria-label': 'Scope',
    },
  });
  try {
    flushSync();
    const radios = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="radio"]'));
    expect(radios.map((radio) => radio.tabIndex)).toEqual([0, -1, -1]);
    radios[0].focus();
    radios[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
    expect(onchange).toHaveBeenLastCalledWith('min');
    expect(document.activeElement).toBe(radios[2]);
    radios[2].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    expect(onchange).toHaveBeenLastCalledWith('one');
    expect(document.activeElement).toBe(radios[0]);
  } finally {
    await unmount(component);
  }
});

it('does not change a disabled segmented control from the keyboard', async () => {
  const onchange = vi.fn();
  const component = mount(SegmentedControl, {
    target: document.body,
    props: {
      options: [
        { value: 'one', label: 'One' },
        { value: 'all', label: 'All' },
      ],
      value: 'one',
      onchange,
      disabled: true,
    },
  });
  try {
    flushSync();
    const radio = document.querySelector<HTMLButtonElement>('[role="radio"]')!;
    expect(radio.disabled).toBe(true);
    radio.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    expect(onchange).not.toHaveBeenCalled();
  } finally {
    await unmount(component);
  }
});
