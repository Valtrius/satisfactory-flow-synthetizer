import { mount, unmount, flushSync } from 'svelte';
import { expect, it } from 'vitest';
import HelpPopover from './HelpPopover.svelte';

it('dismisses help with Escape and an outside pointer without stealing focus', async () => {
  const outside = document.createElement('button');
  document.body.append(outside);
  const component = mount(HelpPopover, {
    target: document.body,
    props: { label: 'Help', sections: [{ title: 'Move', items: ['Drag a node.'] }] },
  });
  try {
    flushSync();
    const trigger = document.querySelector<HTMLButtonElement>('[aria-label="Help"][aria-haspopup]')!;
    trigger.click();
    flushSync();
    expect(document.activeElement?.getAttribute('role')).toBe('dialog');
    document.activeElement!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    flushSync();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(document.activeElement).toBe(trigger);
    trigger.click();
    flushSync();
    outside.focus();
    outside.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }));
    flushSync();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(document.activeElement).toBe(outside);
  } finally {
    await unmount(component);
    outside.remove();
  }
});
