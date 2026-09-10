import { mount, unmount, flushSync } from 'svelte';
import { expect, it, vi } from 'vitest';
import ConfirmDialog from './ConfirmDialog.svelte';

it('opens modally, focuses Cancel, and handles native Escape cancellation', async () => {
  const proto = HTMLDialogElement.prototype;
  const savedShow = Object.getOwnPropertyDescriptor(proto, 'showModal');
  const savedClose = Object.getOwnPropertyDescriptor(proto, 'close');
  const show = vi.fn(function (this: HTMLDialogElement) {
    this.open = true;
  });
  Object.defineProperty(proto, 'showModal', { configurable: true, value: show });
  Object.defineProperty(proto, 'close', {
    configurable: true,
    value() {
      this.open = false;
    },
  });
  const onclose = vi.fn(),
    onconfirm = vi.fn();
  const trigger = document.createElement('button');
  document.body.append(trigger);
  trigger.focus();
  const component = mount(ConfirmDialog, {
    target: document.body,
    props: { title: 'Delete?', description: 'Cannot undo.', onclose, onconfirm },
  });
  try {
    flushSync();
    expect(show).toHaveBeenCalledOnce();
    expect(document.activeElement?.textContent).toBe('Cancel');
    const event = new Event('cancel', { cancelable: true });
    document.querySelector('dialog')!.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(onclose).toHaveBeenCalledOnce();
    expect(onconfirm).not.toHaveBeenCalled();
  } finally {
    await unmount(component);
    expect(document.activeElement).toBe(trigger);
    trigger.remove();
    if (savedShow) Object.defineProperty(proto, 'showModal', savedShow);
    else Reflect.deleteProperty(proto, 'showModal');
    if (savedClose) Object.defineProperty(proto, 'close', savedClose);
    else Reflect.deleteProperty(proto, 'close');
  }
});
