<script lang="ts">
  import { onMount } from 'svelte';
  import Titlebar from '$lib/Titlebar.svelte';
  import Chart from '$lib/Chart.svelte';
  import AppsPanel from '$lib/AppsPanel.svelte';
  import IfacesPanel from '$lib/IfacesPanel.svelte';
  import Controls from '$lib/Controls.svelte';
  import { live, startLive, reconnect, view } from '$lib/live.svelte';
  import HistoryView from '$lib/HistoryView.svelte';
  import { throughputSeries, currentRate, topHosts } from '$lib/live-derive';
  import { formatUnits } from '$lib/format';

  const series = $derived(throughputSeries(live.samples, live.showAll));
  const rate = $derived(currentRate(live.samples, live.showAll));
  const hosts = $derived(topHosts(live.samples));

  // Tras unos segundos sin contacto, dejamos de decir "sigo reintentando" y
  // pasamos a un aviso accionable según la causa. Un parpadeo breve no alarma.
  let offlineMs = $state(0);
  $effect(() => {
    if (live.connected) { offlineMs = 0; return; }
    const t0 = Date.now();
    const id = setInterval(() => { offlineMs = Date.now() - t0; }, 1000);
    return () => clearInterval(id);
  });
  const persistente = $derived(!live.connected && offlineMs >= 5000);

  onMount(() => {
    const unlisten = startLive();
    return () => { unlisten.then((f) => f()); };
  });
</script>

<Titlebar />
<div class="app">
  <main>
    {#if view.mode === 'history'}
      <HistoryView />
    {:else}
      {#if !live.connected}
        <div class="offline">
          {#if !persistente}
            <p>Sin contacto con el daemon. Sigo reintentando — en cuanto vuelva, el caudal aparece solo.</p>
          {:else}
            {#if live.reason === 'denied'}
              <p class="ot">Sin permiso para hablar con el daemon.</p>
              <p>Añádete al grupo <code>caudalghost</code> y vuelve a iniciar sesión.</p>
            {:else if live.reason === 'down'}
              <p class="ot">El daemon no está corriendo.</p>
              <p>Arráncalo con privilegios (sudo): <code>systemctl enable --now caudalghostd</code></p>
            {:else}
              <p class="ot">No consigo contactar el daemon.</p>
              <p>Comprueba que <code>caudalghostd</code> está activo y vuelve a intentarlo.</p>
            {/if}
            <button class="retry" onclick={reconnect}>Reintentar</button>
          {/if}
        </div>
      {:else}
        <section class="hero">
          <div class="hd">
            <span class="lbl">Caudal · ahora</span>
            <span class="rate">
              <span class="rx">↓ {formatUnits(rate.rx, live.units)}/s</span>
              <span class="tx">↑ {formatUnits(rate.tx, live.units)}/s</span>
            </span>
          </div>
          <Chart ts={series.ts} rx={series.rx} tx={series.tx} units={live.units} />
          {#if series.ts.length < 8}<p class="hint">Acumulando caudal…</p>{/if}
        </section>
        <section class="grid">
          <AppsPanel />
          <IfacesPanel />
        </section>
        {#if hosts.length > 0}
          <section class="hosts">
            <span class="lbl">Servidores · ahora</span>
            <div class="hlist">
              {#each hosts as h (h.host)}
                <div class="hrow"><span class="hn">{h.host}</span><span class="hv rx">↓ {formatUnits(h.rx, live.units)}/s</span><span class="hv tx">↑ {formatUnits(h.tx, live.units)}/s</span></div>
              {/each}
            </div>
          </section>
        {/if}
      {/if}
    {/if}
  </main>
  <Controls />
</div>

<style>
  .app { height: calc(100vh - 44px); display: flex; flex-direction: column; }
  main { flex: 1; overflow: auto; padding: 14px; display: flex; flex-direction: column; gap: 14px; }
  .offline { margin: auto; color: var(--cg-text-secondary); font-size: 14px; max-width: 46ch; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 8px; }
  .offline p { margin: 0; }
  .offline .ot { color: var(--cg-text); font-weight: 600; }
  .offline code { font-family: var(--cg-font-mono); font-size: 12px; background: var(--cg-surface); border: 1px solid var(--cg-border); border-radius: 5px; padding: 1px 6px; }
  .offline .retry { margin-top: 4px; font: inherit; font-size: 13px; color: var(--cg-text); background: var(--cg-surface); border: 1px solid var(--cg-border); border-radius: 7px; padding: 6px 16px; cursor: pointer; }
  .offline .retry:hover { border-color: var(--cg-text-tertiary); }
  .hero { display: flex; flex-direction: column; gap: 6px; }
  .hd { display: flex; align-items: baseline; justify-content: space-between; }
  .lbl { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--cg-text-tertiary); }
  .rate { font-family: var(--cg-font-mono); font-variant-numeric: tabular-nums; font-size: 15px; display: flex; gap: 18px; }
  .rate .rx { color: var(--cg-rx-live); } .rate .tx { color: var(--cg-tx); }
  .hint { font-size: 11px; color: var(--cg-text-tertiary); margin: 0; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  @media (max-width: 680px) { .grid { grid-template-columns: 1fr; } }
  .hosts { background: var(--cg-surface); border: 1px solid var(--cg-border); border-radius: 10px; padding: 12px 14px; }
  .hosts .lbl { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--cg-text-tertiary); }
  .hlist { display: flex; flex-direction: column; gap: 2px; margin-top: 8px; }
  .hrow { display: grid; grid-template-columns: 1fr auto auto; align-items: baseline; gap: 12px; }
  .hn { font-family: var(--cg-font-mono); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hv { font-family: var(--cg-font-mono); font-variant-numeric: tabular-nums; font-size: 11px; }
  .hv.rx { color: var(--cg-rx-live); } .hv.tx { color: var(--cg-tx); }
</style>
