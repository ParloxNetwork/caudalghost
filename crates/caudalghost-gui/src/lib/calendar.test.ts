import { expect, test } from 'vitest';
import { monthGrid, isoOf, ymOf, prettyDate, pad2, MONTHS_ES, WEEKDAYS_ES } from './calendar';

test('pad2 rellena a 2 dígitos', () => { expect(pad2(3)).toBe('03'); expect(pad2(12)).toBe('12'); });
test('isoOf usa mes 0-based', () => { expect(isoOf(2026, 6, 2)).toBe('2026-07-02'); });
test('prettyDate dd/mm/yyyy', () => { expect(prettyDate('2026-07-02')).toBe('02/07/2026'); expect(prettyDate('x')).toBe(''); });
test('ymOf parsea o devuelve null', () => { expect(ymOf('2026-07-02')).toEqual({ y: 2026, m: 6 }); expect(ymOf('nope')).toBeNull(); });
test('monthGrid: 42 celdas, arranca en lunes', () => {
  const g = monthGrid(2026, 6); // julio 2026 (1 jul = miércoles)
  expect(g.length).toBe(42);
  expect(g[0]).toEqual({ iso: '2026-06-29', day: 29, inMonth: false });
  expect(g[2]).toEqual({ iso: '2026-07-01', day: 1, inMonth: true });
});
test('monthGrid: febrero bisiesto 2024 tiene 29 días', () => {
  const g = monthGrid(2024, 1);
  expect(g.filter((c) => c.inMonth).length).toBe(29);
  expect(g.some((c) => c.inMonth && c.day === 29)).toBe(true);
});
test('etiquetas ES', () => { expect(MONTHS_ES[6]).toBe('julio'); expect(WEEKDAYS_ES[0]).toBe('lun'); expect(WEEKDAYS_ES.length).toBe(7); });
