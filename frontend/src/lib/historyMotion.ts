import type { Action } from 'svelte/action';
import type { HistoryBand } from './historyModel';
import { prefersReducedMotion } from './pointerReorder';

type Row = { id: string; band: HistoryBand };

/** Keep the keyed each-block stable while only row contents change. */
export function createHistoryOrder(): (keys: readonly string[]) => readonly string[] {
  let previous: readonly string[] = [];
  return (keys) => {
    if (keys.length !== previous.length || keys.some((key, index) => key !== previous[index])) {
      previous = keys;
    }
    return previous;
  };
}

// Read the preference when FLIP starts, not once when the panel is created.
export function historyMotionDuration(): number {
  return prefersReducedMotion() ? 0 : 300;
}

/** One entrance per row ID, on a child of the element owned by FLIP. */
export function createHistoryEntrance(initialIds: Iterable<string>): Action<HTMLElement, Row> {
  const seen = new Set(initialIds);

  return (node, { id, band }) => {
    const firstAppearance = !seen.has(id);
    seen.add(id);
    if (!firstAppearance || band !== 'queued' || prefersReducedMotion()) return;

    const animation = node.animate(
      [
        { opacity: 0, transform: 'translateY(-12px)' },
        { opacity: 1, transform: 'translateY(0)' },
      ],
      { duration: 300, easing: 'cubic-bezier(0.22, 1, 0.36, 1)' },
    );
    const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
    const onPreferenceChange = () => {
      if (preference.matches) animation.cancel();
    };
    const stopListening = () => preference.removeEventListener('change', onPreferenceChange);
    preference.addEventListener('change', onPreferenceChange);
    animation.onfinish = stopListening;
    animation.oncancel = stopListening;

    return {
      // Status and progress updates must not restart or cut short the entrance.
      destroy() {
        stopListening();
        animation.cancel();
      },
    };
  };
}
