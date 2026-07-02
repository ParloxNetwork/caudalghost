<script lang="ts">
  import { history, RANGES, loadHistory } from '$lib/history.svelte';
  import { rangeToken } from '$lib/history-derive';
  import DateField from '$lib/DateField.svelte';
  let mode = $state<'dia' | 'rango'>('dia');
  let day = $state('');
  let desde = $state('');
  let hasta = $state('');
  let hastaOpen = $state(false);
  const token = $derived(rangeToken(desde, hasta));

  // Día: aplicar en cuanto se elige la fecha (el calendario propio se cierra solo).
  function pickDay(iso: string) { day = iso; loadHistory(`dia:${iso}`); }
  // Rango: elegir "Desde" abre solo el calendario de "Hasta".
  function pickDesde(iso: string) { desde = iso; hastaOpen = true; }
  // Rango: elegir "Hasta" aplica el rango si es válido.
  function pickHasta(iso: string) { hasta = iso; const t = rangeToken(desde, iso); if (t) loadHistory(t); }
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
      <DateField value={day} placeholder="Día específico" onpick={pickDay} />
    {:else}
      <DateField value={desde} placeholder="Desde" onpick={pickDesde} />
      <span class="dash">–</span>
      <DateField bind:open={hastaOpen} value={hasta} placeholder="Hasta" onpick={pickHasta} />
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
  .picker { display: flex; align-items: center; gap: 5px; flex-wrap: nowrap; }
  .toggle { display: flex; gap: 2px; }
  .toggle button { padding: 3px 8px; }
  .dash { color: var(--cg-text-tertiary); }
</style>
