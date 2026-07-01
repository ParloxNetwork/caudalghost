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
