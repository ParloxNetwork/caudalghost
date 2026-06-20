export interface SeriePoint { ts: number; rx: number; tx: number; }
export interface TotalRow { iface: string; rx: number; tx: number; }
export interface AppRow { app: string; rx: number; tx: number; }

/** Totales del rango (suma de las filas por interfaz). */
export function historyTotals(total: TotalRow[]) {
  return total.reduce((a, r) => ({ rx: a.rx + r.rx, tx: a.tx + r.tx }), { rx: 0, tx: 0 });
}
