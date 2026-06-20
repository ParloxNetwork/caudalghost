import { expect, test } from 'vitest';
import { historyTotals } from './history-derive';
test('historyTotals suma filas', () => {
  expect(historyTotals([{ iface: 'eth0', rx: 10, tx: 1 }, { iface: 'wlan0', rx: 5, tx: 2 }])).toEqual({ rx: 15, tx: 3 });
});
test('historyTotals vacío', () => { expect(historyTotals([])).toEqual({ rx: 0, tx: 0 }); });
