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
