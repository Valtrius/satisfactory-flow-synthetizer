import { afterEach, expect, it, vi } from 'vitest';
import { createPointerDrag } from './pointerDrag';

let drag: ReturnType<typeof createPointerDrag>;
afterEach(() => drag?.dispose());
function pointer(type: string, id = 1, x = 20): PointerEvent {
  return Object.assign(new Event(type), {
    pointerId: id,
    button: 0,
    isPrimary: true,
    clientX: x,
    clientY: 20,
  }) as PointerEvent;
}
function setup() {
  const callbacks = { move: vi.fn(), finish: vi.fn(), cancel: vi.fn() };
  drag = createPointerDrag(callbacks);
  expect(drag.start(pointer('pointerdown'), { left: 10, top: 10 })).toBe(true);
  return callbacks;
}
it.each(['pointercancel', 'blur', 'Escape', 'dispose'])(
  '%s abandons the gesture without committing or selecting',
  (reason) => {
    const callbacks = setup();
    window.dispatchEvent(pointer('pointermove', 1, 40));
    expect(document.body.classList.contains('list-dragging')).toBe(true);
    if (reason === 'dispose') drag.dispose();
    else if (reason === 'Escape') window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    else window.dispatchEvent(pointer(reason));
    window.dispatchEvent(pointer('pointerup'));
    expect(callbacks.cancel).toHaveBeenCalledOnce();
    expect(callbacks.finish).not.toHaveBeenCalled();
    expect(document.body.classList.contains('list-dragging')).toBe(false);
  },
);
it('ignores other pointers and distinguishes a click from a drag', () => {
  const callbacks = setup();
  window.dispatchEvent(pointer('pointercancel', 2));
  window.dispatchEvent(pointer('pointermove', 2, 90));
  window.dispatchEvent(pointer('pointerup'));
  expect(callbacks.finish).toHaveBeenCalledExactlyOnceWith(false);
  expect(callbacks.move).not.toHaveBeenCalled();
  expect(drag.start(pointer('pointerdown'), { left: 10, top: 10 })).toBe(true);
  window.dispatchEvent(pointer('pointermove', 1, 40));
  window.dispatchEvent(pointer('pointerup'));
  expect(callbacks.move).toHaveBeenCalledExactlyOnceWith({ left: 30, top: 10, clientX: 40, clientY: 20 });
  expect(callbacks.finish).toHaveBeenLastCalledWith(true);
});
