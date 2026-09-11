import { getPlatform } from './platform';
import { getNodesBounds, getSmoothStepPath, type Edge, type Node } from '@xyflow/svelte';
import { parseMultiplier } from './endpoints';
import { handleAnchor, handlePoint, nodeBox, sideToPosition, type PortSide } from './graph';
import type { EndpointRow } from '../types';

const VIEW_PADDING = 48;
const NODE_RADIUS = 8;
const HANDLE_RADIUS = 4;
const BACKGROUND = '#08141c';
const LABEL_FILL = '#e7eff2';
const EDGE_LABEL_FILL = '#d8e5e9';
const EDGE_LABEL_BG = '#0b1922';
const HANDLE_FILL = '#a8c0ca';
const HANDLE_STROKE = '#071017';
const FONT = "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";

const EDGE_STYLES = {
  flow: { stroke: '#4cc9ce', dashed: false, marker: 'arrow-flow' },
  feedback: { stroke: '#e8b654', dashed: true, marker: 'arrow-feedback' },
  discard: { stroke: '#f06a62', dashed: true, marker: 'arrow-discard' },
} as const;

const NODE_STYLES: Record<string, { stroke: string; from: string; to: string }> = {
  input: { stroke: '#2d9aa0', from: '#143d43', to: '#10292f' },
  output: { stroke: '#73a96a', from: '#273f2c', to: '#172a1d' },
  splitter2: { stroke: '#dd762f', from: '#4a2c1a', to: '#2a1d16' },
  splitter3: { stroke: '#dd762f', from: '#4a2c1a', to: '#2a1d16' },
  merger2: { stroke: '#6d7fd0', from: '#292e50', to: '#1a2038' },
  merger3: { stroke: '#6d7fd0', from: '#292e50', to: '#1a2038' },
  discard: { stroke: '#bd5a54', from: '#402320', to: '#281918' },
};

const DEFAULT_NODE_STYLE = {
  stroke: '#456171',
  from: '#18303c',
  to: '#101f29',
};

export function defaultSvgFileName(inputs: EndpointRow[], outputs: EndpointRow[], nodeCount: number): string {
  const inputPart = endpointFilePart(inputs);
  const outputPart = endpointFilePart(outputs);
  const nodes = Number.isFinite(nodeCount) && nodeCount > 0 ? Math.trunc(nodeCount) : 0;
  return `${inputPart}_to_${outputPart}_n-${nodes}.svg`;
}

function endpointFilePart(endpoints: EndpointRow[]): string {
  const parts = endpoints.map(endpointFileToken).filter((part) => part.length > 0);
  return parts.length > 0 ? parts.join('-') : '0';
}

function endpointFileToken(endpoint: EndpointRow): string {
  const rate = filenameRate(endpoint.rate);
  const multiplier = parseMultiplier(endpoint.multiplier);
  return multiplier > 1 ? `${multiplier}x${rate}` : rate;
}

/** Filename-safe rate: integers, terminating decimals, otherwise `npd` for n/d. */
export function filenameRate(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length === 0) return '0';
  const rational = parseRational(trimmed);
  if (rational) {
    const reduced = reduceRational(rational);
    if (reduced.denominator === 1n) return reduced.numerator.toString();
    const decimal = terminatingDecimal(reduced);
    if (decimal !== null) return decimal;
    return `${reduced.numerator}p${reduced.denominator}`;
  }
  return sanitizeFilenameToken(trimmed.replaceAll('/', 'p'));
}

function parseRational(value: string): { numerator: bigint; denominator: bigint } | null {
  if (value.includes('/')) {
    const parts = value.split('/');
    if (parts.length !== 2) return null;
    const numerator = parseSignedInteger(parts[0].trim());
    const denominator = parseSignedInteger(parts[1].trim());
    if (numerator === null || denominator === null || denominator === 0n) {
      return null;
    }
    return { numerator, denominator };
  }
  return parseDecimal(value);
}

function parseSignedInteger(value: string): bigint | null {
  if (!/^[+-]?\d+$/.test(value)) return null;
  return BigInt(value);
}

function parseDecimal(value: string): { numerator: bigint; denominator: bigint } | null {
  if (!/^[+-]?(?:\d+\.?\d*|\.\d+)$/.test(value)) return null;
  const negative = value.startsWith('-');
  const unsigned = value.replace(/^[+-]/, '');
  const [whole = '0', fraction = ''] = unsigned.split('.');
  const digits = `${whole === '' ? '0' : whole}${fraction}`;
  const numerator = BigInt(digits);
  const denominator = 10n ** BigInt(fraction.length);
  return {
    numerator: negative ? -numerator : numerator,
    denominator,
  };
}

function reduceRational(value: { numerator: bigint; denominator: bigint }): {
  numerator: bigint;
  denominator: bigint;
} {
  let { numerator, denominator } = value;
  if (denominator < 0n) {
    numerator = -numerator;
    denominator = -denominator;
  }
  const divisor = gcd(abs(numerator), denominator);
  return {
    numerator: numerator / divisor,
    denominator: denominator / divisor,
  };
}

function terminatingDecimal(value: { numerator: bigint; denominator: bigint }): string | null {
  let twos = 0n;
  let fives = 0n;
  let denominator = value.denominator;
  while (denominator % 2n === 0n) {
    denominator /= 2n;
    twos += 1n;
  }
  while (denominator % 5n === 0n) {
    denominator /= 5n;
    fives += 1n;
  }
  if (denominator !== 1n) return null;
  const places = twos > fives ? twos : fives;
  const scale = 2n ** (places - twos) * 5n ** (places - fives);
  const digits = abs(value.numerator) * scale;
  const padded = digits.toString().padStart(Number(places) + 1, '0');
  const split = padded.length - Number(places);
  const fraction = padded.slice(split).replace(/0+$/, '');
  const whole = padded.slice(0, split);
  const sign = value.numerator < 0n ? '-' : '';
  if (fraction.length === 0) return `${sign}${whole}`;
  return `${sign}${whole}.${fraction}`;
}

function gcd(left: bigint, right: bigint): bigint {
  let a = left;
  let b = right;
  while (b !== 0n) {
    const rest = a % b;
    a = b;
    b = rest;
  }
  return a;
}

function abs(value: bigint): bigint {
  return value < 0n ? -value : value;
}

function sanitizeFilenameToken(value: string): string {
  return value
    .replaceAll(/[<>:"/\\|?*\u0000-\u001f]/g, '')
    .replaceAll(/\s+/g, '')
    .replaceAll(/\.+$/g, '');
}

export function escapeXml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;');
}

export function graphToSvg(nodes: Node[], edges: Edge[]): string {
  const bounds = nodes.length > 0 ? getNodesBounds(nodes) : { x: 0, y: 0, width: 1, height: 1 };
  const minX = bounds.x - VIEW_PADDING;
  const minY = bounds.y - VIEW_PADDING;
  const width = Math.max(1, bounds.width + VIEW_PADDING * 2);
  const height = Math.max(1, bounds.height + VIEW_PADDING * 2);
  const kinds = [...new Set(nodes.map(nodeKind))];
  const edgeMarkup = edges.map((edge) => edgeMarkupParts(edge, nodes));
  const body = [
    `<rect x="${fmt(minX)}" y="${fmt(minY)}" width="${fmt(width)}" height="${fmt(height)}" fill="${BACKGROUND}"/>`,
    ...edgeMarkup.flatMap((parts) => parts.path),
    ...nodes.flatMap(nodeElements),
    ...edgeMarkup.flatMap((parts) => parts.label),
  ];

  return [
    `<?xml version="1.0" encoding="UTF-8"?>`,
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${fmt(minX)} ${fmt(minY)} ${fmt(width)} ${fmt(height)}" width="${fmt(width)}" height="${fmt(height)}" font-family="${escapeXml(FONT)}">`,
    `<defs>`,
    ...kinds.map(gradientDef),
    ...Object.values(EDGE_STYLES).map((style) => arrowMarker(style.marker, style.stroke)),
    `</defs>`,
    ...body,
    `</svg>`,
  ].join('');
}

export async function saveSvgFile(contents: string, fileName: string): Promise<void> {
  await getPlatform().files.saveText({ contents, fileName, format: 'svg' });
}

function nodeKind(node: Node): string {
  const match = String(node.class ?? '').match(/factory-node--([a-z0-9]+)/);
  return match?.[1] ?? 'default';
}

function nodeStyle(kind: string): { stroke: string; from: string; to: string } {
  return NODE_STYLES[kind] ?? DEFAULT_NODE_STYLE;
}

function gradientDef(kind: string): string {
  const style = nodeStyle(kind);
  return [
    `<linearGradient id="node-fill-${escapeXml(kind)}" x1="0%" y1="0%" x2="100%" y2="100%">`,
    `<stop offset="0%" stop-color="${style.from}"/>`,
    `<stop offset="100%" stop-color="${style.to}"/>`,
    `</linearGradient>`,
  ].join('');
}

function arrowMarker(id: string, color: string): string {
  return [
    `<marker id="${id}" markerWidth="18" markerHeight="18" refX="16" refY="9" orient="auto" markerUnits="userSpaceOnUse">`,
    `<polygon points="2,2 16,9 2,16" fill="${color}"/>`,
    `</marker>`,
  ].join('');
}

function nodeElements(node: Node): string[] {
  const box = nodeBox(node);
  const kind = nodeKind(node);
  const style = nodeStyle(kind);
  const label = nodeLabel(node);
  const handles = occupiedSides(node).map((side) => {
    const point = handlePoint(box, side);
    return `<circle cx="${fmt(point.x)}" cy="${fmt(point.y)}" r="${HANDLE_RADIUS}" fill="${HANDLE_FILL}" stroke="${HANDLE_STROKE}" stroke-width="1"/>`;
  });
  return [
    `<rect x="${fmt(box.x)}" y="${fmt(box.y)}" width="${fmt(box.width)}" height="${fmt(box.height)}" rx="${NODE_RADIUS}" ry="${NODE_RADIUS}" fill="url(#node-fill-${escapeXml(kind)})" stroke="${style.stroke}" stroke-width="1.5"/>`,
    `<text x="${fmt(box.x + box.width / 2)}" y="${fmt(box.y + box.height / 2)}" text-anchor="middle" dominant-baseline="middle" fill="${LABEL_FILL}" font-size="16" font-weight="700">${escapeXml(label)}</text>`,
    ...handles,
  ];
}

function occupiedSides(node: Node): PortSide[] {
  const inputs = (node.data.inputPositions as PortSide[] | undefined) ?? [];
  const outputs = (node.data.outputPositions as PortSide[] | undefined) ?? [];
  const sides = [...new Set([...inputs, ...outputs])];
  if (sides.length > 0) return sides;
  if (node.type === 'input') return ['right'];
  if (node.type === 'output') return ['left'];
  return ['left', 'right'];
}

function nodeLabel(node: Node): string {
  const label = node.data.label;
  return typeof label === 'string' || typeof label === 'number' ? String(label) : '';
}

function edgeKind(edge: Edge): keyof typeof EDGE_STYLES {
  const className = String(edge.class ?? '');
  if (className.includes('factory-edge--discard')) return 'discard';
  if (className.includes('factory-edge--feedback')) return 'feedback';
  return 'flow';
}

function edgeLabelText(edge: Edge): string {
  return typeof edge.label === 'string' || typeof edge.label === 'number' ? String(edge.label) : '';
}

function edgeMarkupParts(edge: Edge, nodes: Node[]): { path: string[]; label: string[] } {
  const source = nodes.find((node) => node.id === edge.source);
  const target = nodes.find((node) => node.id === edge.target);
  if (!source || !target) return { path: [], label: [] };
  const start = handleAnchor(source, 'source', edge.sourceHandle);
  const end = handleAnchor(target, 'target', edge.targetHandle);
  const [path, labelX, labelY] = getSmoothStepPath({
    sourceX: start.point.x,
    sourceY: start.point.y,
    sourcePosition: sideToPosition(start.side),
    targetX: end.point.x,
    targetY: end.point.y,
    targetPosition: sideToPosition(end.side),
  });
  const style = EDGE_STYLES[edgeKind(edge)];
  const dash = style.dashed ? ` stroke-dasharray="7 5"` : '';
  const pathEl = `<path d="${escapeXml(path)}" fill="none" stroke="${style.stroke}" stroke-width="2.2" stroke-linecap="round"${dash} marker-end="url(#${style.marker})"/>`;
  const label = edgeLabelText(edge);
  if (!label) return { path: [pathEl], label: [] };
  const labelWidth = Math.max(24, label.length * 6.6 + 12);
  const labelHeight = 16;
  return {
    path: [pathEl],
    label: [
      `<rect x="${fmt(labelX - labelWidth / 2)}" y="${fmt(labelY - labelHeight / 2)}" width="${fmt(labelWidth)}" height="${fmt(labelHeight)}" rx="2" fill="${EDGE_LABEL_BG}" fill-opacity="0.94"/>`,
      `<text x="${fmt(labelX)}" y="${fmt(labelY)}" text-anchor="middle" dominant-baseline="middle" fill="${EDGE_LABEL_FILL}" font-size="10" font-weight="700">${escapeXml(label)}</text>`,
    ],
  };
}

function fmt(value: number): string {
  return Number.parseFloat(value.toFixed(2)).toString();
}
