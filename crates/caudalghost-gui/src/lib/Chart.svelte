<script lang="ts">
  import uPlot from 'uplot';
  import 'uplot/dist/uPlot.min.css';
  import { untrack } from 'svelte';
  import { formatUnits, type UnitMode } from './format';

  let { ts, rx, tx, units, interactive = false }:
    { ts: number[]; rx: number[]; tx: number[]; units: UnitMode; interactive?: boolean } = $props();
  let wrapEl: HTMLDivElement;
  let el: HTMLDivElement;
  let tipEl: HTMLDivElement;
  let plot: uPlot | undefined;

  function spanSecs(): number { return ts.length > 1 ? ts[ts.length - 1] - ts[0] : 0; }
  function fmtTime(sec: number): string {
    const d = new Date(sec * 1000);
    const sp = spanSecs();
    if (sp > 3 * 86400) return d.toLocaleDateString('es', { month: 'short', day: 'numeric' });
    if (sp > 2 * 3600) return d.toLocaleString('es', { day: 'numeric', hour: '2-digit', minute: '2-digit' });
    return d.toLocaleTimeString('es', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  function makeOpts(w: number): uPlot.Options {
    return {
      width: w,
      height: 220,
      cursor: { y: false, drag: { x: interactive, y: false, setScale: interactive } },
      legend: { show: false },
      scales: { x: { time: true } },
      axes: [
        { stroke: '#5A7079', grid: { stroke: '#1D2D34', width: 1 }, ticks: { stroke: '#1D2D34' }, size: 32 },
        { stroke: '#5A7079', grid: { stroke: '#1D2D34', width: 1 }, ticks: { stroke: '#1D2D34' }, size: 60,
          values: (_u, vals) => vals.map((v) => formatUnits(v, units)) },
      ],
      series: [
        {},
        { label: 'rx', stroke: '#2DD4BF', width: 2, fill: 'rgba(45,212,191,0.18)' },
        { label: 'tx', stroke: '#F59E0B', width: 2, fill: 'rgba(245,158,11,0.12)' },
      ],
      hooks: {
        setCursor: [
          (u) => {
            // uPlot types cursor.idx and cursor.left as number | null | undefined at runtime
            const idx = u.cursor.idx as number | null | undefined;
            const left = (u.cursor.left as number | null | undefined) ?? -1;
            if (idx == null || left < 0 || !tipEl || !wrapEl) { if (tipEl) tipEl.style.display = 'none'; return; }
            const xv = (u.data[0] as number[])[idx];
            const rxv = ((u.data[1] as (number | null)[])[idx] ?? 0) as number;
            const txv = ((u.data[2] as (number | null)[])[idx] ?? 0) as number;
            const o = u.over.getBoundingClientRect();
            const wr = wrapEl.getBoundingClientRect();
            tipEl.style.display = 'flex';
            tipEl.style.left = `${o.left - wr.left + left}px`;
            tipEl.style.top = `${o.top - wr.top + ((u.cursor.top as number | null | undefined) ?? 0)}px`;
            // DOM seguro en vez de innerHTML: textContent escapa por construcción,
            // así ninguna salida de fmtTime/formatUnits puede interpretarse como HTML.
            const span = (cls: string, text: string): HTMLSpanElement => {
              const s = document.createElement('span');
              s.className = cls;
              s.textContent = text;
              return s;
            };
            tipEl.replaceChildren(
              span('tt', fmtTime(xv)),
              span('tr', `↓ ${formatUnits(rxv, units)}`),
              span('tx', `↑ ${formatUnits(txv, units)}`),
            );
          },
        ],
      },
    };
  }

  function makeData(): uPlot.AlignedData {
    return ts.length ? [ts, rx, tx] : [[0], [0], [0]];
  }

  // init: crea/recrea el plot solo ante cambios de units/interactive (re-aplica formateo/cursor).
  $effect(() => {
    void units; void interactive;
    if (!el) return;
    plot?.destroy();
    const p = new uPlot(makeOpts(el.clientWidth || 600), untrack(() => makeData()), el);
    plot = p;
    const ro = new ResizeObserver(() => p.setSize({ width: el.clientWidth, height: 220 }));
    ro.observe(el);
    return () => { ro.disconnect(); p.destroy(); plot = undefined; if (tipEl) tipEl.style.display = 'none'; };
  });

  // datos: actualiza sin recrear (setData resetea el zoom al cambiar de rango, lo cual es deseable).
  $effect(() => {
    void ts; void rx; void tx;
    plot?.setData(makeData());
  });
</script>

<div class="chartwrap" bind:this={wrapEl}>
  <div class="chart" bind:this={el}></div>
  <div class="tip" bind:this={tipEl}></div>
</div>

<style>
  .chartwrap { position: relative; width: 100%; }
  .chart { width: 100%; }
  :global(.u-wrap) { font-family: var(--cg-font-mono); font-size: 10px; }
  .tip {
    position: absolute; display: none; transform: translate(-50%, calc(-100% - 8px));
    pointer-events: none; z-index: 6; gap: 8px;
    background: var(--cg-surface-elev); border: 1px solid var(--cg-border); border-radius: 7px;
    padding: 4px 8px; font-family: var(--cg-font-mono); font-size: 10px; white-space: nowrap;
    box-shadow: 0 4px 14px #0008;
  }
  .tip :global(.tt) { color: var(--cg-text-secondary); }
  .tip :global(.tr) { color: var(--cg-rx); }
  .tip :global(.tx) { color: var(--cg-tx); }
</style>
