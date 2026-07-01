<script lang="ts">
  import { history, RANGES, loadHistory } from '$lib/history.svelte';
  import { rangeToken } from '$lib/history-derive';
  let mode = $state<'dia' | 'rango'>('dia');
  let day = $state('');
  let desde = $state('');
  let hasta = $state('');
  function pickDay() { if (day) loadHistory(`dia:${day}`); }
  const token = $derived(rangeToken(desde, hasta));
  function applyRange() { if (token) loadHistory(token); }
</script>

<div class="chips">
  {#each RANGES.slice(0, 7) as r (r.token)}
    <button class:on={history.range === r.token} onclick={() => loadHistory(r.token)}>{r.label}</button>
  {/each}
  <span class="sep"></span>
  {#each RANGES.slice(7) as r (r.token)}
    <button class="cal" class:on={history.range === r.token} onclick={() => loadHistory(r.token)}>{r.label}</button>
  {/each}
  <span class="sep"></span>
  <div class="picker">
    <div class="toggle">
      <button class:on={mode === 'dia'} onclick={() => (mode = 'dia')}>Día</button>
      <button class:on={mode === 'rango'} onclick={() => (mode = 'rango')}>Rango</button>
    </div>
    {#if mode === 'dia'}
      <input type="date" bind:value={day} onchange={pickDay} aria-label="Día específico" />
    {:else}
      <input type="date" bind:value={desde} aria-label="Desde" />
      <span class="dash">–</span>
      <input type="date" bind:value={hasta} aria-label="Hasta" />
      <button class="apply" disabled={!token} onclick={applyRange}>Ver</button>
    {/if}
  </div>
</div>

<style>
  .chips { display: flex; align-items: center; gap: 5px; flex-wrap: wrap; padding: 10px 14px; border-bottom: 1px solid var(--cg-border); }
  button { font: inherit; font-size: 11px; padding: 3px 10px; border: 1px solid var(--cg-border); border-radius: 6px; background: var(--cg-surface); color: var(--cg-text-secondary); cursor: pointer; }
  button:hover { color: var(--cg-text); }
  button.on { background: var(--cg-accent); color: var(--cg-on-accent); border-color: var(--cg-accent); font-weight: 600; }
  button.cal { color: var(--cg-accent-bright); }
  button.cal.on { color: var(--cg-on-accent); }
  button:disabled { opacity: 0.4; cursor: default; }
  .sep { width: 1px; height: 16px; background: var(--cg-border); margin: 0 3px; }
  .picker { display: flex; align-items: center; gap: 5px; }
  .toggle { display: flex; gap: 2px; }
  .toggle button { padding: 3px 8px; }
  .dash { color: var(--cg-text-tertiary); }
  input[type="date"] { font: inherit; font-size: 11px; background: var(--cg-surface); color: var(--cg-text-secondary); border: 1px solid var(--cg-border); border-radius: 6px; padding: 2px 6px; color-scheme: dark; }
</style>
