<script lang="ts">
  import { live, cycleUnits, togglePause, toggleShowAll } from '$lib/live.svelte';
  import { unitLabel } from '$lib/format';
  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLElement && ['INPUT', 'TEXTAREA'].includes(e.target.tagName)) return;
    if (e.key === 'u') cycleUnits();
    else if (e.key === 'p') togglePause();
    else if (e.key === 'a') toggleShowAll();
  }
</script>

<svelte:window onkeydown={onKey} />

<footer class="controls">
  <button onclick={cycleUnits}>unidades: <b>{unitLabel(live.units)}</b></button>
  <button class:active={live.paused} onclick={togglePause}>{live.paused ? '▶ reanudar' : '⏸ pausa'}</button>
  <button class:active={live.showAll} onclick={toggleShowAll}>incluir lo</button>
  <span class="sig"><span class="ver">v{__APP_VERSION__}</span> | Hecho con <span class="h">❤</span> por Parlox Network</span>
</footer>

<style>
  .controls { display: flex; align-items: center; gap: 8px; padding: 8px 14px; border-top: 1px solid var(--cg-border); }
  button { font: inherit; font-size: 11px; padding: 4px 10px; border: 1px solid var(--cg-border); border-radius: 6px; background: var(--cg-surface); color: var(--cg-text-secondary); cursor: pointer; }
  button:hover { color: var(--cg-text); }
  button.active { background: var(--cg-accent); color: var(--cg-on-accent); border-color: var(--cg-accent); font-weight: 600; }
  button b { color: var(--cg-accent); }
  .sig { margin-left: auto; font-size: 11px; color: var(--cg-text-tertiary); }
  .sig .h { color: var(--cg-parlox-signature); }
  .sig .ver { font-family: var(--cg-font-mono); }
</style>
