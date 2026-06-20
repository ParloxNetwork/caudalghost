<script lang="ts">
  import { onMount } from 'svelte';
  import RangeChips from '$lib/RangeChips.svelte';
  import HistoryBreakdown from '$lib/HistoryBreakdown.svelte';
  import HostsBreakdown from '$lib/HostsBreakdown.svelte';
  import Chart from '$lib/Chart.svelte';
  import { history, loadHistory, historyTotals, RANGES } from '$lib/history.svelte';
  import { live } from '$lib/live.svelte';
  import { formatUnits } from '$lib/format';

  const totals = $derived(historyTotals(history.total));
  const rangeLabel = $derived(RANGES.find((r) => r.token === history.range)?.label ?? history.range);
  const ts = $derived(history.series.map((p) => p.ts));
  const rx = $derived(history.series.map((p) => p.rx));
  const tx = $derived(history.series.map((p) => p.tx));

  onMount(() => { if (history.total.length === 0 && !history.loading) loadHistory(history.range); });
</script>

<RangeChips />
<div class="hist">
  <div class="summary">
    {#if history.loading}
      <span class="mut">Cargando…</span>
    {:else if history.error}
      <span class="err">No pude leer ese rango: {history.error}</span>
    {:else}
      <span class="rng">{rangeLabel}</span>
      <span class="tot"><span class="rx">↓ {formatUnits(totals.rx, live.units)}</span><span class="tx">↑ {formatUnits(totals.tx, live.units)}</span></span>
    {/if}
  </div>
  {#if history.series.length === 0 && !history.loading && !history.error}
    <p class="empty">Aún no hay caudal registrado en este rango. En cuanto fluya algo, quedará aquí para siempre.</p>
  {:else}
    <Chart {ts} {rx} {tx} units={live.units} interactive />
  {/if}
  <HistoryBreakdown />
  <HostsBreakdown />
</div>

<style>
  .hist { display: flex; flex-direction: column; gap: 14px; padding: 14px; }
  .summary { display: flex; align-items: baseline; gap: 14px; font-family: var(--cg-font-mono); font-variant-numeric: tabular-nums; font-size: 13px; }
  .rng { color: var(--cg-text-secondary); }
  .tot { display: flex; gap: 16px; }
  .tot .rx { color: var(--cg-rx); } .tot .tx { color: var(--cg-tx); }
  .mut { color: var(--cg-text-tertiary); } .err { color: var(--cg-error); }
  .empty { color: var(--cg-text-secondary); font-size: 13px; max-width: 50ch; }
</style>
