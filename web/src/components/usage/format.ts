export function parseFiniteNumber(value: unknown): number | null {
  if (typeof value === 'number') return Number.isFinite(value) ? value : null;
  if (typeof value === 'string') {
    const n = Number.parseFloat(value);
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

export function fmtInt(value: unknown, fallback = '--'): string {
  const n = parseFiniteNumber(value);
  if (n === null) return fallback;
  return new Intl.NumberFormat('zh-CN').format(Math.round(n));
}

export function fmtUsd(value: unknown, digits: number, fallback = '--'): string {
  const n = parseFiniteNumber(value);
  if (n === null) return fallback;
  return `$${n.toFixed(digits)}`;
}

export function fmtTokenK(value: number): string {
  if (value >= 1000) return `${(value / 1000).toFixed(1)}k`;
  return String(value);
}

export function fmtTimestamp(ts: number): string {
  return new Date(ts * 1000).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
  });
}

export function fmtDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}
