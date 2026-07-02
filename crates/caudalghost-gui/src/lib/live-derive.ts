export interface IfaceDelta { iface: string; rx: number; tx: number; }
export interface AppDelta { app: string; rx: number; tx: number; }
export interface AppRemoteDelta { app: string; host: string; rx: number; tx: number; }
export interface Sample { ts: number; deltas: IfaceDelta[]; apps: AppDelta[]; flows?: AppRemoteDelta[]; }

const keep = (iface: string, showAll: boolean) => showAll || iface !== 'lo';

/** Serie por-segundo del caudal total (rx/tx) sobre la ventana. */
export function throughputSeries(samples: Sample[], showAll: boolean) {
  const ts: number[] = [], rx: number[] = [], tx: number[] = [];
  for (const s of samples) {
    let r = 0, t = 0;
    for (const d of s.deltas) if (keep(d.iface, showAll)) { r += d.rx; t += d.tx; }
    ts.push(s.ts); rx.push(r); tx.push(t);
  }
  return { ts, rx, tx };
}

/** Tasa actual = último sample (bytes/s con interval_secs=1). */
export function currentRate(samples: Sample[], showAll: boolean) {
  const last = samples.at(-1);
  if (!last) return { rx: 0, tx: 0 };
  let rx = 0, tx = 0;
  for (const d of last.deltas) if (keep(d.iface, showAll)) { rx += d.rx; tx += d.tx; }
  return { rx, tx };
}

/** Top apps agregadas sobre la ventana, desc por total. */
export function topApps(samples: Sample[], limit = 10) {
  const acc = new Map<string, { rx: number; tx: number }>();
  for (const s of samples) for (const a of s.apps) {
    const e = acc.get(a.app) ?? { rx: 0, tx: 0 };
    e.rx += a.rx; e.tx += a.tx; acc.set(a.app, e);
  }
  return [...acc].map(([app, v]) => ({ app, rx: v.rx, tx: v.tx }))
    .sort((a, b) => (b.rx + b.tx) - (a.rx + a.tx)).slice(0, limit);
}

/** Top hosts remotos (FQDN) agregados sobre la ventana, desc por total. */
export function topHosts(samples: Sample[], limit = 10) {
  const acc = new Map<string, { rx: number; tx: number }>();
  for (const s of samples) for (const f of s.flows ?? []) {
    const e = acc.get(f.host) ?? { rx: 0, tx: 0 };
    e.rx += f.rx; e.tx += f.tx; acc.set(f.host, e);
  }
  return [...acc].map(([host, v]) => ({ host, rx: v.rx, tx: v.tx }))
    .sort((a, b) => (b.rx + b.tx) - (a.rx + a.tx)).slice(0, limit);
}

/** Filas por interfaz: total rx/tx sobre la ventana + serie rx+tx por segundo (alineada). */
export function ifaceRows(samples: Sample[], showAll: boolean) {
  const tot = new Map<string, { rx: number; tx: number; serie: number[] }>();
  samples.forEach((s, i) => {
    for (const d of s.deltas) if (keep(d.iface, showAll)) {
      const e = tot.get(d.iface) ?? { rx: 0, tx: 0, serie: [] };
      e.rx += d.rx; e.tx += d.tx; tot.set(d.iface, e);
    }
    for (const [, e] of tot) { while (e.serie.length < i) e.serie.push(0); }
    for (const [iface, e] of tot) {
      const d = s.deltas.find((x) => x.iface === iface);
      e.serie.push(d ? d.rx + d.tx : 0);
    }
  });
  return [...tot].map(([iface, e]) => ({ iface, rx: e.rx, tx: e.tx, serie: e.serie }))
    .sort((a, b) => (b.rx + b.tx) - (a.rx + a.tx));
}
