import { expect, test } from 'vitest';
import { historyTotals } from './history-derive';
test('historyTotals suma filas', () => {
  expect(historyTotals([{ iface: 'eth0', rx: 10, tx: 1 }, { iface: 'wlan0', rx: 5, tx: 2 }])).toEqual({ rx: 15, tx: 3 });
});
test('historyTotals vacío', () => { expect(historyTotals([])).toEqual({ rx: 0, tx: 0 }); });

import { resetHistory } from './history-derive';
test('resetHistory vacía series/total/apps/hosts in-place', () => {
  const h = { series: [{ ts: 1, rx: 2, tx: 3 }], total: [{ iface: 'eth0', rx: 1, tx: 1 }], apps: [{ app: 'x', rx: 1, tx: 1 }], hosts: [{ host: 'y', rx: 1, tx: 1 }] };
  resetHistory(h);
  expect(h).toEqual({ series: [], total: [], apps: [], hosts: [] });
});

import { rangeToken, prettyRangeLabel } from './history-derive';
const RS = [{ token: '7d', label: '7 días' }, { token: 'ayer', label: 'ayer' }];

test('rangeToken válido e inválido', () => {
  expect(rangeToken('2026-06-01', '2026-06-05')).toBe('rango:2026-06-01:2026-06-05');
  expect(rangeToken('2026-06-05', '2026-06-01')).toBeNull(); // desde > hasta
  expect(rangeToken('', '2026-06-05')).toBeNull();
  expect(rangeToken('2026-06-01', '2026-06-01')).toBe('rango:2026-06-01:2026-06-01'); // un día vale
});

test('prettyRangeLabel', () => {
  expect(prettyRangeLabel('7d', RS)).toBe('7 días');
  expect(prettyRangeLabel('dia:2026-06-01', RS)).toBe('2026-06-01');
  expect(prettyRangeLabel('rango:2026-06-01:2026-06-05', RS)).toBe('2026-06-01 – 2026-06-05');
  expect(prettyRangeLabel('desconocido', RS)).toBe('desconocido');
});
