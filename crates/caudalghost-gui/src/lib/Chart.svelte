<script lang="ts">
  import uPlot from 'uplot';
  import 'uplot/dist/uPlot.min.css';
  import { untrack } from 'svelte';
  import { formatUnits, type UnitMode } from './format';
  import { chooseTier, fmtFine, fmtCoarse, coarseMidpoints, fineAxisLabels } from './axis-time';
  import { clampTooltip } from './chart-geom';
  import { liveTicks, liveLabel, smoothNow } from './live-axis';

  let { ts, rx, tx, units, interactive = false, xmin = undefined, xmax = undefined, flow = false }:
    { ts: number[]; rx: number[]; tx: number[]; units: UnitMode; interactive?: boolean; xmin?: number; xmax?: number; flow?: boolean } = $props();
  let wrapEl: HTMLDivElement;
  let el: HTMLDivElement;
  let tipEl: HTMLDivElement;
  let plot: uPlot | undefined;
  let latestTs = 0;
  let arrivalPerf = 0;
  let flowW = 60;
  let rafId = 0;
  const reducedMotion = () =>
    typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  function spanSecs(): number { return ts.length > 1 ? ts[ts.length - 1] - ts[0] : 0; }
  function fmtTime(sec: number): string {
    const d = new Date(sec * 1000);
    const sp = spanSecs();
    if (interactive) {
      const tier = chooseTier(sp);
      return tier === 'intraday' ? `${fmtCoarse(sec, tier)} ${fmtFine(sec, tier)}` : fmtCoarse(sec, tier);
    }
    if (sp > 3 * 86400) return d.toLocaleDateString('es', { month: 'short', day: 'numeric' });
    if (sp > 2 * 3600) return d.toLocaleString('es', { day: 'numeric', hour: '2-digit', minute: '2-digit' });
    return d.toLocaleTimeString('es', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  function makeOpts(w: number): uPlot.Options {
    const h = interactive ? 240 : 220;
    const axesInteractive: uPlot.Axis[] = [
      // Nivel fino (X, tiempo): reformatea cada tick con el tier del span visible.
      // `incrs` de minuto-para-arriba: evita ticks sub-minuto que duplicarían "HH:MM" en la vista de 5 min.
      { scale: 'x', stroke: '#5A7079', grid: { stroke: '#1D2D34', width: 1 }, ticks: { stroke: '#1D2D34' }, size: 28,
        incrs: [60, 300, 600, 900, 1800, 3600, 7200, 14400, 21600, 43200, 86400, 172800, 604800, 2592000, 7776000, 31536000],
        values: (u, splits) => {
          const s = u.scales.x; const tier = chooseTier(((s.max ?? 0) - (s.min ?? 0)));
          return fineAxisLabels(splits as number[], tier);
        } },
      // Eje Y (unidades).
      { scale: 'y', stroke: '#5A7079', grid: { stroke: '#1D2D34', width: 1 }, ticks: { stroke: '#1D2D34' }, size: 60,
        values: (_u, vals) => vals.map((v) => formatUnits(v, units)) },
      // Nivel grueso (X, fechas): etiquetas centradas bajo cada día/mes/año, sin grid ni ticks.
      { scale: 'x', side: 2, stroke: '#5A7079', grid: { show: false }, ticks: { show: false }, size: 16, gap: 2,
        splits: (u, _ai, min, max) => coarseMidpoints(min, max, chooseTier(max - min)),
        values: (u, splits) => {
          const s = u.scales.x; const tier = chooseTier(((s.max ?? 0) - (s.min ?? 0)));
          return splits.map((v) => fmtCoarse(v as number, tier));
        } },
    ];
    const axesDefault: uPlot.Axis[] = [
      { stroke: '#5A7079', grid: { stroke: '#1D2D34', width: 1 }, ticks: { stroke: '#1D2D34' }, size: 32 },
      { stroke: '#5A7079', grid: { stroke: '#1D2D34', width: 1 }, ticks: { stroke: '#1D2D34' }, size: 60,
        values: (_u, vals) => vals.map((v) => formatUnits(v, units)) },
    ];
    const axesFlow: uPlot.Axis[] = [
      { scale: 'x', stroke: '#5A7079', grid: { stroke: '#1D2D34', width: 1 }, ticks: { stroke: '#1D2D34' }, size: 28,
        splits: (u, _ai, min, max) => liveTicks(min, max),
        values: (u, splits) => splits.map((v) => liveLabel(v as number, u.scales.x.max ?? 0)) },
      { scale: 'y', stroke: '#5A7079', grid: { stroke: '#1D2D34', width: 1 }, ticks: { stroke: '#1D2D34' }, size: 60,
        values: (_u, vals) => vals.map((v) => formatUnits(v, units)) },
    ];
    const axes = interactive ? axesInteractive : flow ? axesFlow : axesDefault;
    return {
      width: w,
      height: h,
      cursor: { y: false, drag: { x: interactive, y: false, setScale: interactive } },
      legend: { show: false },
      scales: { x: { time: true } },
      axes,
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
            const px = o.left - wr.left + left;
            const py = o.top - wr.top + ((u.cursor.top as number | null | undefined) ?? 0);
            const { left: cx, flipY } = clampTooltip(px, py, tipEl.offsetWidth, tipEl.offsetHeight, o.width);
            tipEl.style.left = `${cx}px`;
            tipEl.style.top = `${py}px`;
            tipEl.style.transform = flipY ? 'translate(-50%, 8px)' : 'translate(-50%, calc(-100% - 8px))';
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
    void units; void interactive; void flow;
    if (!el) return;
    plot?.destroy();
    const p = new uPlot(makeOpts(el.clientWidth || 600), untrack(() => makeData()), el);
    plot = p;
    const h = interactive ? 240 : 220;
    const ro = new ResizeObserver(() => p.setSize({ width: el.clientWidth, height: h }));
    ro.observe(el);
    if (flow && !reducedMotion()) {
      const tick = () => {
        if (plot && latestTs) {
          const now = smoothNow(latestTs, (performance.now() - arrivalPerf) / 1000, 1.25);
          plot.setScale('x', { min: now - flowW, max: now });
        }
        rafId = requestAnimationFrame(tick);
      };
      rafId = requestAnimationFrame(tick);
    }
    return () => { cancelAnimationFrame(rafId); rafId = 0; ro.disconnect(); p.destroy(); plot = undefined; if (tipEl) tipEl.style.display = 'none'; };
  });

  // datos: actualiza sin recrear. En histórico ancla el eje; en flow lo maneja el rAF.
  $effect(() => {
    void ts; void rx; void tx; void xmin; void xmax; void flow;
    if (!plot) return;
    const pinned = interactive && xmin != null && xmax != null && xmax > xmin;
    const flowing = flow && !reducedMotion();
    plot.setData(makeData(), !(pinned || flowing)); // pinned/flowing: no auto-rescale
    if (pinned) plot.setScale('x', { min: xmin as number, max: xmax as number });
    if (flowing && ts.length) {
      latestTs = ts[ts.length - 1];
      arrivalPerf = performance.now();
      flowW = ts.length > 1 ? ts[ts.length - 1] - ts[0] : 60;
    }
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
