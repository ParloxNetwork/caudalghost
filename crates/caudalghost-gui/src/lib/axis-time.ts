// Única fuente de verdad del formato temporal del eje X y del tooltip del histórico.
// Determinista (no depende de ICU): usa MONTHS_ES fijo y getters locales de Date.
export type Tier = 'intraday' | 'days' | 'months';

export const MONTHS_ES = ['ene', 'feb', 'mar', 'abr', 'may', 'jun', 'jul', 'ago', 'sep', 'oct', 'nov', 'dic'];
const DAY = 86400;
const pad2 = (n: number) => String(n).padStart(2, '0');

/** Elige el tier a partir del span visible (en segundos). */
export function chooseTier(spanSecs: number): Tier {
  if (spanSecs <= 2 * DAY) return 'intraday';
  if (spanSecs <= 120 * DAY) return 'days';
  return 'months';
}

/** Etiqueta del nivel fino (fila superior, junto al plot). */
export function fmtFine(sec: number, tier: Tier): string {
  const d = new Date(sec * 1000);
  if (tier === 'intraday') return `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
  if (tier === 'days') return String(d.getDate());
  return MONTHS_ES[d.getMonth()];
}

/** Etiqueta del nivel grueso (fila inferior, tenue, agrupa fechas). */
export function fmtCoarse(sec: number, tier: Tier): string {
  const d = new Date(sec * 1000);
  if (tier === 'intraday') return `${d.getDate()} ${MONTHS_ES[d.getMonth()]}`;
  if (tier === 'days') return MONTHS_ES[d.getMonth()];
  return String(d.getFullYear());
}

/** Unidad de bucketing del nivel grueso según el tier. */
export function coarseUnit(tier: Tier): 'day' | 'month' | 'year' {
  if (tier === 'intraday') return 'day';
  if (tier === 'days') return 'month';
  return 'year';
}

function startOf(sec: number, unit: 'day' | 'month' | 'year'): number {
  const d = new Date(sec * 1000);
  d.setHours(0, 0, 0, 0);
  if (unit === 'month') d.setDate(1);
  if (unit === 'year') { d.setMonth(0, 1); }
  return Math.floor(d.getTime() / 1000);
}

function advance(sec: number, unit: 'day' | 'month' | 'year'): number {
  const d = new Date(sec * 1000);
  if (unit === 'day') d.setDate(d.getDate() + 1);
  else if (unit === 'month') d.setMonth(d.getMonth() + 1);
  else d.setFullYear(d.getFullYear() + 1);
  return Math.floor(d.getTime() / 1000);
}

/** Inicios de bucket (local) que solapan [start, end). */
export function coarseBuckets(startSec: number, endSec: number, unit: 'day' | 'month' | 'year'): number[] {
  const out: number[] = [];
  let cur = startOf(startSec, unit);
  // Cap de seguridad: nunca más de ~1000 buckets (protege de ventanas absurdas).
  for (let i = 0; cur < endSec && i < 1000; i++) {
    out.push(cur);
    cur = advance(cur, unit);
  }
  return out;
}

/** Punto medio (recortado a [min,max]) de cada bucket: dónde centrar la etiqueta de fecha. */
export function coarseMidpoints(minSec: number, maxSec: number, tier: Tier): number[] {
  const unit = coarseUnit(tier);
  const starts = coarseBuckets(minSec, maxSec, unit);
  return starts.map((s) => {
    const next = advance(s, unit);
    const from = Math.max(s, minSec);
    const to = Math.min(next, maxSec);
    return (from + to) / 2;
  });
}
