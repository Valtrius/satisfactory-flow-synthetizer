import { setListDragging } from './pointerReorder';

export type DragFrame = { left: number; top: number; clientX: number; clientY: number };

/** Own one pointer gesture. Cancellation never invokes the drop callback. */
export function createPointerDrag(callbacks: {
  move: (frame: DragFrame) => void;
  finish: (active: boolean) => void;
  cancel: () => void;
}) {
  let pointer: number | null = null;
  let active = false;
  let originX = 0,
    originY = 0,
    grabX = 0,
    grabY = 0;

  function cleanup(): void {
    pointer = null;
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', finish);
    window.removeEventListener('pointercancel', cancelled);
    window.removeEventListener('blur', cancel);
    window.removeEventListener('keydown', keydown);
    setListDragging(false);
  }
  function move(event: PointerEvent): void {
    if (event.pointerId !== pointer) return;
    if (!active && Math.abs(event.clientX - originX) <= 4 && Math.abs(event.clientY - originY) <= 4) return;
    active = true;
    setListDragging(true);
    callbacks.move({
      left: event.clientX - grabX,
      top: event.clientY - grabY,
      clientX: event.clientX,
      clientY: event.clientY,
    });
  }
  function finish(event: PointerEvent): void {
    if (event.pointerId !== pointer) return;
    const dragged = active;
    cleanup();
    callbacks.finish(dragged);
  }
  function cancelled(event: PointerEvent): void {
    if (event.pointerId === pointer) cancel();
  }
  function keydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      cancel();
    }
  }
  function cancel(): void {
    if (pointer == null) return;
    cleanup();
    callbacks.cancel();
  }
  return {
    start(event: PointerEvent, rect: Pick<DOMRect, 'left' | 'top'>): boolean {
      if (pointer != null || event.button !== 0 || event.isPrimary === false) return false;
      pointer = event.pointerId;
      active = false;
      originX = event.clientX;
      originY = event.clientY;
      grabX = originX - rect.left;
      grabY = originY - rect.top;
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', finish);
      window.addEventListener('pointercancel', cancelled);
      window.addEventListener('blur', cancel);
      window.addEventListener('keydown', keydown);
      return true;
    },
    cancel,
    dispose: cancel,
  };
}
