import { invoke } from '@tauri-apps/api/core';
export type { SeriePoint, TotalRow, AppRow } from './history-derive';
export { historyTotals } from './history-derive';

import type { SeriePoint, TotalRow, AppRow } from './history-derive';
import { resetHistory } from './history-derive';

export type RemoteRow = { host: string; rx: number; tx: number };

export const RANGES = [
  { token: '5m', label: '5 min' }, { token: '1h', label: '1 h' }, { token: '24h', label: '24 h' },
  { token: '7d', label: '7 días' }, { token: '30d', label: '30 d' }, { token: '90d', label: '90 d' }, { token: '365d', label: '1 año' },
  { token: 'hoy', label: 'hoy' }, { token: 'ayer', label: 'ayer' }, { token: 'este-mes', label: 'este mes' }, { token: 'este-anio', label: 'este año' },
];

export const history = $state({
  range: '7d', loading: false, error: '',
  series: [] as SeriePoint[], total: [] as TotalRow[], apps: [] as AppRow[], hosts: [] as RemoteRow[],
  start: 0, end: 0,
});

let reqId = 0;
export async function loadHistory(range: string) {
  const id = ++reqId;
  history.range = range; history.loading = true; history.error = '';
  resetHistory(history); // no dejar el rango anterior pintándose mientras llega el nuevo
  try {
    const d = await invoke<{ series: SeriePoint[]; total: TotalRow[]; apps: AppRow[]; hosts: RemoteRow[]; start: number; end: number }>('history', { rango: range });
    if (id !== reqId) return; // una petición más nueva ya está en curso
    history.series = d.series; history.total = d.total; history.apps = d.apps; history.hosts = d.hosts ?? [];
    history.start = d.start; history.end = d.end;
  } catch (e) {
    if (id !== reqId) return;
    history.error = String(e); history.series = []; history.total = []; history.apps = []; history.hosts = [];
  } finally {
    if (id === reqId) history.loading = false;
  }
}
