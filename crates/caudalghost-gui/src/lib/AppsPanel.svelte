<script lang="ts">
  import { live } from '$lib/live.svelte';
  import { topApps } from '$lib/live-derive';
  import { formatUnits } from '$lib/format';
  const apps = $derived(topApps(live.samples));
  const secs = $derived(Math.max(1, Math.min(60, live.samples.length)));
  const max = $derived(Math.max(1, ...apps.map((a) => a.rx + a.tx)));
</script>

<section class="panel">
  <div class="lbl">top apps · ahora</div>
  {#if !live.ebpf}
    <p class="warn">Sin eBPF no puedo ver qué app consume. Arranca el daemon con privilegios (sudo) y volverán a aparecer.</p>
  {:else if apps.length === 0}
    <p class="empty">Sin actividad de apps ahora mismo.</p>
  {:else}
    <div class="rows">
      {#each apps as a, i (a.app)}
        <div class="row" class:first={i === 0}>
          <span class="nm">{a.app}</span>
          <span class="bar"><i style="width:{((a.rx + a.tx) / max) * 100}%"></i></span>
          <span class="v rx">↓ {formatUnits(a.rx / secs, live.units)}/s</span>
          <span class="v tx">↑ {formatUnits(a.tx / secs, live.units)}/s</span>
        </div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .panel { background: var(--cg-surface); border: 1px solid var(--cg-border); border-radius: 10px; padding: 12px 14px; min-width: 0; }
  .lbl { font-size: 10px; text-transform: uppercase; letter-spacing: .06em; color: var(--cg-text-tertiary); margin-bottom: 8px; }
  .warn { color: var(--cg-warn); font-size: 12px; line-height: 1.4; }
  .empty { color: var(--cg-text-tertiary); font-size: 12px; }
  .rows { display: flex; flex-direction: column; gap: 3px; }
  .row { display: grid; grid-template-columns: 90px 1fr auto auto; align-items: center; gap: 8px; font-size: 12px; color: var(--cg-text); }
  .row.first { font-weight: 700; }
  .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bar { height: 5px; background: var(--cg-bg); border-radius: 3px; overflow: hidden; }
  .bar i { display: block; height: 100%; background: linear-gradient(90deg, var(--cg-accent-deep), var(--cg-accent)); }
  .v { font-family: var(--cg-font-mono); font-variant-numeric: tabular-nums; font-size: 11px; }
  .v.rx { color: var(--cg-rx); } .v.tx { color: var(--cg-tx); }
</style>
