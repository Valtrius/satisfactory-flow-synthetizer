import { createRawSnippet, flushSync, mount, unmount } from 'svelte';
import { afterEach, expect, it, vi } from 'vitest';
import Workbench from './Workbench.svelte';

afterEach(() => vi.unstubAllGlobals());

function setup(graphFullscreen = false) {
  let wide = true;
  let change: () => void = () => {};
  vi.stubGlobal('matchMedia', (query: string) => ({
    get matches() {
      return query === '(min-width: 1800px)' && wide;
    },
    addEventListener: (_: string, listener: () => void) => (change = listener),
    removeEventListener: vi.fn(),
  }));
  const snippet = (html: string) => createRawSnippet(() => ({ render: () => html }));
  const component = mount(Workbench, {
    target: document.body,
    props: {
      graphFullscreen,
      setup: snippet('<input aria-label="Draft" value="60" />'),
      history: snippet('<input aria-label="History filter" />'),
      layouts: snippet('<div data-test="layouts">Layouts</div>'),
      children: snippet('<div data-test="graph">Graph</div>'),
    },
  });
  flushSync();
  return {
    component,
    compact(value: boolean) {
      wide = !value;
      change();
      flushSync();
    },
  };
}

function toggle(label: string) {
  document.querySelector<HTMLButtonElement>(`[aria-label="Toggle ${label}"]`)!.click();
  flushSync();
}

it('hiding and reopening panels retains draft, filter, layouts and graph instances', async () => {
  const { component } = setup();
  try {
    const draft = document.querySelector<HTMLInputElement>('[aria-label="Draft"]')!;
    const filter = document.querySelector<HTMLInputElement>('[aria-label="History filter"]')!;
    const layouts = document.querySelector('[data-test="layouts"]');
    const graph = document.querySelector('[data-test="graph"]');
    draft.value = '144';
    filter.value = 'saved run';
    for (const label of ['Setup', 'History', 'Layouts']) toggle(label);
    for (const label of ['Layouts', 'Setup', 'History']) toggle(label);
    expect(document.querySelector('[aria-label="Draft"]')).toBe(draft);
    expect(draft.value).toBe('144');
    expect(filter.value).toBe('saved run');
    expect(document.querySelector('[data-test="layouts"]')).toBe(layouts);
    expect(document.querySelector('[data-test="graph"]')).toBe(graph);
  } finally {
    await unmount(component);
  }
});

function press(code: string, key: string, options: KeyboardEventInit = {}, target: EventTarget = window) {
  const event = new KeyboardEvent('keydown', { code, key, bubbles: true, cancelable: true, ...options });
  target.dispatchEvent(event);
  flushSync();
  return event;
}

function panelState() {
  return ['Setup', 'History', 'Layouts'].map(
    (label) => document.querySelector(`section[aria-label="${label}"]`)?.getAttribute('aria-hidden') === 'false',
  );
}

it.each([
  ['Digit1', '1', 0],
  ['Digit2', '2', 1],
  ['Digit3', '3', 2],
  ['Digit1', '&', 0],
  ['Digit2', 'é', 1],
  ['Digit3', '"', 2],
] as const)('toggles only the panel for physical %s with character %s', async (code, key, index) => {
  const { component } = setup();
  try {
    expect(press(code, key).defaultPrevented).toBe(true);
    expect(panelState()).toEqual([0, 1, 2].map((panel) => panel !== index));
    press(code, key);
    expect(panelState()).toEqual([true, true, true]);
    // Shift is allowed because it produces the digit on layouts such as French AZERTY.
    press(code, String(index + 1), { shiftKey: true });
    expect(panelState()[index]).toBe(false);
  } finally {
    await unmount(component);
  }
});

it('keeps History and Layouts shortcuts independent in the combined column', async () => {
  const { component, compact } = setup();
  try {
    compact(true);
    press('Digit2', 'é');
    expect(panelState()).toEqual([true, false, true]);
    press('Digit3', '"');
    expect(panelState()).toEqual([true, false, false]);
    press('Digit2', 'é');
    expect(panelState()).toEqual([true, true, false]);
    compact(false);
    expect(panelState()).toEqual([true, true, false]);
  } finally {
    await unmount(component);
  }
});

it('ignores numpad keys, modifiers, repeats, composition and already handled events', async () => {
  const { component } = setup();
  try {
    for (const code of ['Numpad1', 'Numpad2', 'Numpad3', 'KeyQ']) expect(press(code, '1').defaultPrevented).toBe(false);
    for (const options of [
      { ctrlKey: true },
      { altKey: true },
      { metaKey: true },
      { repeat: true },
      { isComposing: true },
      { location: 3 },
    ])
      expect(press('Digit1', '&', options).defaultPrevented).toBe(false);
    const handled = new KeyboardEvent('keydown', { code: 'Digit1', key: '&', cancelable: true });
    handled.preventDefault();
    window.dispatchEvent(handled);
    flushSync();
    expect(panelState()).toEqual([true, true, true]);
  } finally {
    await unmount(component);
  }
});

it('leaves typing and popup interactions alone', async () => {
  const { component } = setup();
  const controls = [
    '<input />',
    '<textarea></textarea>',
    '<select></select>',
    '<div contenteditable="true"><span></span></div>',
    '<div role="textbox"><span></span></div>',
    '<div role="combobox"><span></span></div>',
    '<div role="dialog"><button></button></div>',
    '<div role="menu"><button></button></div>',
    '<div popover><button></button></div>',
  ];
  try {
    for (const html of controls) {
      const wrapper = document.createElement('div');
      wrapper.innerHTML = html;
      document.body.append(wrapper);
      try {
        const target = wrapper.firstElementChild!.firstElementChild ?? wrapper.firstElementChild!;
        expect(press('Digit1', '&', {}, target).defaultPrevented).toBe(false);
        expect(panelState()).toEqual([true, true, true]);
      } finally {
        wrapper.remove();
      }
    }
  } finally {
    await unmount(component);
  }
});

it('disables panel shortcuts while the graph is fullscreen', async () => {
  const { component } = setup(true);
  try {
    for (const code of ['Digit1', 'Digit2', 'Digit3']) expect(press(code, '1').defaultPrevented).toBe(false);
    expect(panelState()).toEqual([true, true, true]);
  } finally {
    await unmount(component);
  }
});

it('combines History and Layouts without losing independent wide-screen selections', async () => {
  const { component, compact } = setup();
  try {
    toggle('Layouts');
    compact(true);
    expect(document.querySelector('[aria-label="Toggle History and Layouts"]')?.getAttribute('aria-pressed')).toBe(
      'true',
    );
    expect(document.querySelector('section[aria-label="Layouts"]')?.getAttribute('aria-hidden')).toBe('true');
    compact(false);
    expect(document.querySelector('[aria-label="Toggle History"]')?.getAttribute('aria-pressed')).toBe('true');
    expect(document.querySelector('[aria-label="Toggle Layouts"]')?.getAttribute('aria-pressed')).toBe('false');
    compact(true);
    toggle('History and Layouts');
    compact(false);
    expect(document.querySelector('[aria-label="Toggle History"]')?.getAttribute('aria-pressed')).toBe('false');
    expect(document.querySelector('[aria-label="Toggle Layouts"]')?.getAttribute('aria-pressed')).toBe('false');
  } finally {
    await unmount(component);
  }
});
