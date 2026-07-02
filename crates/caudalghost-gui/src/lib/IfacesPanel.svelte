<script lang="ts">
  import { live } from '$lib/live.svelte';
  import { ifaceRows } from '$lib/live-derive';
  import { formatUnits } from '$lib/format';
  const rows = $derived(ifaceRows(live.samples, live.showAll));
  const secs = $derived(Math.max(1, Math.min(60, live.samples.length)));
  const BARS = '▁▂▃▄▅▆▇█';
  function spark(serie: number[]): string {
    const m = Math.max(1, ...serie);
    return serie.slice(-12).map((v) => BARS[Math.min(7, Math.floor((v / m) * 7))]).join('');
  }
</script>

<section class="panel">
  <div class="lbl">interfaces · ahora</div>
  <div class="scroll">
  <div class="rows">
    {#each rows as r, i (r.iface)}
      <div class="row" class:first={i === 0}>
        <span class="nm">{r.iface}</span>
        <span class="v rx">↓ {formatUnits(r.rx / secs, live.units)}/s</span>
        <span class="v tx">↑ {formatUnits(r.tx / secs, live.units)}/s</span>
        <span class="spark">{spark(r.serie)}</span>
      </div>
      {#if live.ifaceIps[r.iface]}
        <div class="ip">{live.ifaceIps[r.iface]}</div>
      {/if}
    {/each}
  </div>
  </div>
</section>

<style>
  .panel { background: var(--cg-surface); border: 1px solid var(--cg-border); border-radius: 10px; padding: 12px 14px; min-width: 0; position: relative; overflow: hidden; min-height: 180px; }
  .lbl { font-size: 10px; text-transform: uppercase; letter-spacing: .06em; color: var(--cg-text-tertiary); margin-bottom: 8px; }
  /* Scroll interno: las interfaces nunca agrandan el layout; Apps manda la altura. */
  .scroll { position: absolute; left: 14px; right: 8px; top: 32px; bottom: 12px; overflow-y: auto; }
  .rows { display: flex; flex-direction: column; gap: 2px; }
  .row { display: grid; grid-template-columns: 80px auto auto 1fr; align-items: center; gap: 10px; font-size: 12px; color: var(--cg-text); }
  .row.first { font-weight: 700; }
  .v { font-family: var(--cg-font-mono); font-variant-numeric: tabular-nums; font-size: 11px; }
  .v.rx { color: var(--cg-rx); } .v.tx { color: var(--cg-tx); }
  .spark { font-family: var(--cg-font-mono); color: var(--cg-accent); letter-spacing: -1px; text-align: right; }
  .ip { font-family: var(--cg-font-mono); font-size: 10px; color: var(--cg-text-tertiary); padding-left: 8px; margin: 0 0 3px; }
</style>
