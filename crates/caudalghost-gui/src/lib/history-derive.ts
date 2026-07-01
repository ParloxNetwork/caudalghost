export interface SeriePoint { ts: number; rx: number; tx: number; }
export interface TotalRow { iface: string; rx: number; tx: number; }
export interface AppRow { app: string; rx: number; tx: number; }

/** Totales del rango (suma de las filas por interfaz). */
export function historyTotals(total: TotalRow[]) {
  return total.reduce((a, r) => ({ rx: a.rx + r.rx, tx: a.tx + r.tx }), { rx: 0, tx: 0 });
}

/** Vacía las listas del estado histórico in-place (para no dejar datos stale al cambiar de rango). */
export function resetHistory(h: { series: unknown[]; total: unknown[]; apps: unknown[]; hosts: unknown[] }): void {
  h.series = [];
  h.total = [];
  h.apps = [];
  h.hosts = [];
}

/** Token de rango de fechas, o null si el rango es inválido (falta fecha o desde > hasta). */
export function rangeToken(desde: string, hasta: string): string | null {
  if (!desde || !hasta || desde > hasta) return null; // ISO YYYY-MM-DD compara cronológicamente
  return `rango:${desde}:${hasta}`;
}

/** Etiqueta legible del rango activo para el encabezado. */
export function prettyRangeLabel(token: string, ranges: { token: string; label: string }[]): string {
  const chip = ranges.find((r) => r.token === token);
  if (chip) return chip.label;
  if (token.startsWith('dia:')) return token.slice(4);
  if (token.startsWith('rango:')) return token.slice(6).replace(':', ' – ');
  return token;
}
