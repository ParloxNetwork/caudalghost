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
