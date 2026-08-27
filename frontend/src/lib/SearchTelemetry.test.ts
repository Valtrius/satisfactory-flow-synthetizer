// @vitest-environment node
import { render } from 'svelte/server';
import type { ComponentProps } from 'svelte';
import { describe, expect, it } from 'vitest';
import SearchTelemetry from './SearchTelemetry.svelte';
import { searchStageView } from './searchStage';

const props: ComponentProps<typeof SearchTelemetry> = {
  searchView: searchStageView(null),
  muted: false,
  busy: true,
  elapsedLabel: '0:01.2',
  headline: 'Enumerating layouts at N = 3',
  subline: 'Collecting every layout at the minimum size.',
  sizeBody: 'Searching N=3.',
  foundCount: 2,
  showFound: true,
  showDetails: true,
  collapsible: true
};

function renderTelemetry(overrides: Partial<ComponentProps<typeof SearchTelemetry>> = {}): string {
  return render(SearchTelemetry, { props: { ...props, ...overrides } }).body;
}

describe('SearchTelemetry', () => {
  it('groups metric cards, profile counts, diagnostics, and collapsed found counts', () => {
    const searchView = {
      ...searchStageView(null),
      lowerBound: 1234,
      nodeCount: 5678,
      custom: ['custom.completed_profiles', 'custom.total_profiles'].map((name, index) => ({
        name, label: name, value: { type: 'integer' as const, value: index ? '20000' : '10000' }, unit: null
      }))
    };
    const html = renderTelemetry({ searchView, foundCount: 12345, detailsExpanded: true });
    for (const formatted of ["1'234", "5'678", "12'345", "10'000 / 20'000"]) {
      expect(html).toContain(formatted);
    }
    expect(renderTelemetry({ foundCount: 12345 })).toContain("12'345");
  });

  it.each([true, false])('defaults to collapsed with busy=%s, keeping the search summary visible', (busy) => {
    const html = renderTelemetry({ busy });

    expect(html).toContain('Show telemetry');
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain(props.headline);
    expect(html).toContain(props.subline);
    expect(html).toContain(props.elapsedLabel);
    expect(html.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ')).toContain('2 found');
    expect(html).not.toContain('Exact search');
    expect(html).not.toContain('Solver diagnostics');
  });

  it.each([true, false])('can show details with busy=%s even without reported diagnostics', (busy) => {
    const html = renderTelemetry({ busy, detailsExpanded: true });
    const controlsId = html.match(/aria-controls="([^"]+)"/)?.[1];

    expect(html).toContain('Hide telemetry');
    expect(html).toContain('aria-expanded="true"');
    expect(controlsId).toBeDefined();
    expect(html).toContain(`id="${controlsId}"`);
    expect(html).toContain('Exact search');
    expect(html).toContain('Solver diagnostics');
    expect(html).toContain('No diagnostics reported yet.');
    expect(html).toContain('Searching N=3.');
  });

  it('keeps single-optimal searches expanded without adding a toggle', () => {
    const html = renderTelemetry({ collapsible: false });

    expect(html).toContain('Exact search');
    expect(html).toContain('Solver diagnostics');
    expect(html).not.toContain('Show telemetry');
    expect(html).not.toContain('Hide telemetry');
  });

  it('omits both the toggle and details when details are unavailable', () => {
    const html = renderTelemetry({ showDetails: false, detailsExpanded: true });

    expect(html).not.toContain('telemetry');
    expect(html).not.toContain('Exact search');
  });
});
