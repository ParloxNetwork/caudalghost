import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import type { Sample } from './live-derive';
import type { UnitMode } from './format';

/** Por qué no responde el daemon (espeja ipc::ConnReason del backend). */
export type ConnReason = 'ok' | 'down' | 'denied' | 'other';

interface LiveUpdate { connected: boolean; ebpf: boolean; version: string; samples: Sample[]; iface_ips: Record<string, string>; reason: ConnReason; }

export type { UnitMode };

export const live = $state({
  connected: false, ebpf: false, version: '',
  samples: [] as Sample[], ifaceIps: {} as Record<string, string>,
  units: 'binary' as UnitMode, paused: false, showAll: false,
  reason: 'other' as ConnReason,
});

export function cycleUnits() { live.units = live.units === 'binary' ? 'decimal' : live.units === 'decimal' ? 'bits' : 'binary'; }
export function togglePause() { live.paused = !live.paused; }
export function toggleShowAll() { live.showAll = !live.showAll; }

function apply(p: LiveUpdate) {
  live.connected = p.connected; live.ebpf = p.ebpf; live.version = p.version;
  live.samples = p.samples; live.ifaceIps = p.iface_ips ?? {};
  live.reason = p.reason ?? 'other';
}

export function startLive() {
  return listen<LiveUpdate>('live-update', (e) => {
    if (live.paused) return;
    apply(e.payload);
  });
}

/** Reintento inmediato (botón "Reintentar"): sondea el daemon ya, sin esperar al loop. */
export async function reconnect() {
  try {
    apply(await invoke<LiveUpdate>('reconnect'));
  } catch {
    /* el loop de fondo sigue reintentando solo */
  }
}

export const view = $state({ mode: 'live' as 'live' | 'history' });
