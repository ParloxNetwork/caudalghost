<script lang="ts">
  import { history } from '$lib/history.svelte';
  import { live } from '$lib/live.svelte';
  import { formatUnits } from '$lib/format';

  const CAT = ['var(--cg-cat-1)', 'var(--cg-cat-2)', 'var(--cg-cat-3)', 'var(--cg-cat-4)', 'var(--cg-cat-5)'];
  const catColor = (i: number) => (i < CAT.length ? CAT[i] : 'var(--cg-cat-other)');

  type Row = { name: string; rx: number; tx: number };
  function withPct(items: Row[], limit = 10) {
    const sorted = [...items].sort((a, b) => (b.rx + b.tx) - (a.rx + a.tx)).slice(0, limit);
    const total = sorted.reduce((s, r) => s + r.rx + r.tx, 0) || 1;
    return sorted.map((r, i) => ({ ...r, pct: ((r.rx + r.tx) / total) * 100, color: catColor(i) }));
  }
  const hosts = $derived(withPct(history.hosts.map((h) => ({ name: h.host, rx: h.rx, tx: h.tx }))));
</script>

<section class="panel">
  <div class="lbl">Hosts · histórico</div>
  {#if hosts.length > 0}
    <div class="stack">{#each hosts as h (h.name)}<i style="background:{h.color};width:{h.pct}%"></i>{/each}</div>
    {#each hosts as h (h.name)}
      <div class="row"><span class="sw" style="background:{h.color}"></span><span class="nm">{h.name}</span><span class="v rx">↓ {formatUnits(h.rx, live.units)}</span><span class="v tx">↑ {formatUnits(h.tx, live.units)}</span><span class="pct">{h.pct.toFixed(0)}%</span></div>
    {/each}
  {:else if !live.ebpf}
    <p class="warn">Sin eBPF no puedo ver con qué hosts se habla. Arranca el daemon con privilegios (sudo).</p>
  {:else}
    <p class="empty">Aún no hay hosts registrados en este rango. En cuanto fluya tráfico DNS resuelto, aparecerán aquí.</p>
  {/if}
</section>

<style>
  .panel { background: var(--cg-surface); border: 1px solid var(--cg-border); border-radius: 10px; padding: 12px 14px; min-width: 0; }
  .lbl { font-size: 10px; text-transform: uppercase; letter-spacing: .06em; color: var(--cg-text-tertiary); margin-bottom: 8px; }
  .warn { color: var(--cg-warn); font-size: 12px; line-height: 1.4; }
  .empty { color: var(--cg-text-tertiary); font-size: 12px; }
  .stack { display: flex; height: 7px; border-radius: 4px; overflow: hidden; margin-bottom: 9px; }
  .stack i { display: block; height: 100%; }
  .row { display: grid; grid-template-columns: 10px 1fr auto auto 32px; align-items: center; gap: 8px; font-size: 12px; color: var(--cg-text); padding: 2px 0; }
  .sw { width: 9px; height: 9px; border-radius: 2px; }
  .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--cg-font-mono); font-size: 11px; }
  .v { font-family: var(--cg-font-mono); font-variant-numeric: tabular-nums; font-size: 11px; }
  .v.rx { color: var(--cg-rx); } .v.tx { color: var(--cg-tx); }
  .pct { font-family: var(--cg-font-mono); font-size: 10px; color: var(--cg-text-tertiary); text-align: right; }
</style>
