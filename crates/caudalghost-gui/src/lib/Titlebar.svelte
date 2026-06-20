<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { live, view } from '$lib/live.svelte';
  const win = getCurrentWindow();
</script>

<header class="tb" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <svg viewBox="0 0 100 120" width="18" height="22" aria-hidden="true">
      <path d="M50 16 C53 29 73 50 73 74 a23 24 0 1 1 -46 0 C27 50 47 29 50 16 Z" fill="none" stroke="#2DD4BF" stroke-width="3"/>
      <circle cx="50" cy="71" r="6" fill="#5EEAD4"/>
    </svg>
    <span class="wm">Caudal<span class="g">Ghost</span></span>
  </div>
  <nav class="seg">
    <button class:on={view.mode === 'live'} onclick={() => (view.mode = 'live')}>En vivo</button>
    <button class:on={view.mode === 'history'} onclick={() => (view.mode = 'history')}>Histórico</button>
  </nav>
  <div class="status" data-tauri-drag-region>
    <span class="dot" class:ok={live.connected} class:err={!live.connected}></span>
    {#if live.connected}eBPF: {live.ebpf ? 'activo' : 'inactivo'}{:else}desconectado — reintentando{/if}
  </div>
  <div class="wc">
    <button onclick={() => win.minimize()} aria-label="Minimizar">—</button>
    <button onclick={() => win.toggleMaximize()} aria-label="Maximizar">▢</button>
    <button class="close" onclick={() => win.close()} aria-label="Cerrar">✕</button>
  </div>
</header>

<style>
  .tb { display: flex; align-items: center; gap: 12px; height: 44px; padding: 0 8px 0 12px;
        background: var(--cg-titlebar); border-bottom: 1px solid var(--cg-border); user-select: none; }
  .brand { display: flex; align-items: center; gap: 8px; }
  .wm { font-family: var(--cg-font-display); font-weight: 700; font-size: 14px; color: var(--cg-text); }
  .wm .g { color: var(--cg-accent); }
  .seg { display: flex; gap: 2px; background: var(--cg-bg); border: 1px solid var(--cg-border); border-radius: 7px; padding: 2px; }
  .seg button { font: inherit; font-size: 10px; padding: 2px 9px; border: 0; border-radius: 5px; background: transparent; color: var(--cg-text-secondary); cursor: pointer; }
  .seg button:hover { color: var(--cg-text); }
  .seg .on { background: var(--cg-accent); color: var(--cg-on-accent); font-weight: 600; }
  .status { margin-left: auto; display: flex; align-items: center; gap: 6px; font-size: 11px; color: var(--cg-text-secondary); }
  .dot { width: 7px; height: 7px; border-radius: 50%; }
  .dot.ok { background: var(--cg-ok); box-shadow: 0 0 6px var(--cg-ok); }
  .dot.err { background: var(--cg-error); box-shadow: 0 0 6px var(--cg-error); }
  .wc { display: flex; gap: 2px; }
  .wc button { width: 30px; height: 28px; border: 0; background: transparent; color: var(--cg-text-secondary); border-radius: 6px; cursor: pointer; font-size: 12px; }
  .wc button:hover { background: var(--cg-surface); color: var(--cg-text); }
  .wc .close:hover { background: var(--cg-error); color: #fff; }
</style>
