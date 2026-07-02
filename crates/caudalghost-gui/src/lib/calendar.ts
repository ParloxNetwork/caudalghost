// Lógica pura del calendario (sin runes) — testeable en entorno node.

export const MONTHS_ES = [
  'enero', 'febrero', 'marzo', 'abril', 'mayo', 'junio',
  'julio', 'agosto', 'septiembre', 'octubre', 'noviembre', 'diciembre',
];
export const WEEKDAYS_ES = ['lun', 'mar', 'mié', 'jue', 'vie', 'sáb', 'dom'];

export function pad2(n: number): string { return n < 10 ? `0${n}` : `${n}`; }

/** ISO local 'YYYY-MM-DD' a partir de año, mes (0-based) y día. */
export function isoOf(y: number, m: number, d: number): string { return `${y}-${pad2(m + 1)}-${pad2(d)}`; }

export interface DayCell { iso: string; day: number; inMonth: boolean; }

/** Rejilla de 6 semanas (42 celdas), lunes primero, para el mes (0-based) dado. */
export function monthGrid(y: number, m: number): DayCell[] {
  const firstDow = new Date(y, m, 1).getDay(); // 0=domingo … 6=sábado
  const lead = (firstDow + 6) % 7;             // días desde el lunes anterior
  const cells: DayCell[] = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(y, m, 1 - lead + i);
    cells.push({ iso: isoOf(d.getFullYear(), d.getMonth(), d.getDate()), day: d.getDate(), inMonth: d.getMonth() === m });
  }
  return cells;
}

/** {y, m(0-based)} de un ISO 'YYYY-MM-DD', o null si no matchea. */
export function ymOf(s: string): { y: number; m: number } | null {
  const mt = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s);
  return mt ? { y: +mt[1], m: +mt[2] - 1 } : null;
}

/** Fecha de hoy en ISO local. */
export function todayISO(): string {
  const n = new Date();
  return isoOf(n.getFullYear(), n.getMonth(), n.getDate());
}

/** 'dd/mm/yyyy' para mostrar en el disparador; '' si el ISO no es válido. */
export function prettyDate(s: string): string {
  const mt = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s);
  return mt ? `${mt[3]}/${mt[2]}/${mt[1]}` : '';
}
