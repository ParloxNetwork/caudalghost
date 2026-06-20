import { expect, test } from 'vitest';
import { throughputSeries, currentRate, topApps, topHosts, ifaceRows, type Sample } from './live-derive';

const S: Sample[] = [
  { ts: 1, deltas: [{ iface: 'eth0', rx: 10, tx: 5 }, { iface: 'lo', rx: 99, tx: 99 }], apps: [{ app: 'firefox', rx: 8, tx: 4 }], flows: [{ app: 'firefox', host: 'github.com', rx: 5, tx: 2 }] },
  { ts: 2, deltas: [{ iface: 'eth0', rx: 20, tx: 5 }, { iface: 'wlan0', rx: 100, tx: 50 }], apps: [{ app: 'firefox', rx: 12, tx: 6 }], flows: [{ app: 'firefox', host: 'github.com', rx: 3, tx: 1 }, { app: 'spotify', host: 'scdn.co', rx: 30, tx: 0 }] },
];
test('throughput excluye lo', () => { expect(throughputSeries(S, false).rx).toEqual([10, 120]); });
test('throughput incluye lo con showAll', () => { expect(throughputSeries(S, true).rx).toEqual([109, 120]); });
test('currentRate último sample', () => { expect(currentRate(S, false)).toEqual({ rx: 120, tx: 55 }); });
test('topApps agrega', () => { expect(topApps(S)[0]).toEqual({ app: 'firefox', rx: 20, tx: 10 }); });
test('topHosts agrega flows', () => {
  const h = topHosts(S);
  expect(h.find((x) => x.host === 'github.com')).toEqual({ host: 'github.com', rx: 8, tx: 3 });
  expect(h[0]).toEqual({ host: 'scdn.co', rx: 30, tx: 0 }); // mayor total primero
});
test('topHosts tolera samples sin flows', () => { expect(topHosts([{ ts: 1, deltas: [], apps: [] }])).toEqual([]); });
test('ifaceRows alinea wlan0 tardía', () => {
  const r = ifaceRows(S, false);
  const wlan = r.find((x) => x.iface === 'wlan0')!;
  expect(wlan.serie).toEqual([0, 150]);
});
