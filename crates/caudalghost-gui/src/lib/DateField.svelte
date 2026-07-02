<script lang="ts">
  import { MONTHS_ES, WEEKDAYS_ES, monthGrid, ymOf, prettyDate, todayISO } from '$lib/calendar';

  let {
    value = '',
    placeholder = 'fecha',
    max = todayISO(),
    open = $bindable(false),
    onpick,
  }: {
    value?: string;
    placeholder?: string;
    max?: string;
    open?: boolean;
    onpick: (iso: string) => void;
  } = $props();

  let root: HTMLDivElement | undefined = $state();
  // Mes/año que se muestran; navegables sin tocar el valor.
  let viewY = $state(2026);
  let viewM = $state(0);

  function syncView() {
    const p = ymOf(value) ?? ymOf(todayISO())!;
    viewY = p.y; viewM = p.m;
  }
  function toggle() { open = !open; }
  function prevMonth() { if (viewM === 0) { viewM = 11; viewY--; } else viewM--; }
  function nextMonth() { if (viewM === 11) { viewM = 0; viewY++; } else viewM++; }
  function prevYear() { viewY--; }
  function nextYear() { viewY++; }
  function choose(iso: string, disabled: boolean) {
    if (disabled) return;
    open = false;
    onpick(iso);
  }

  const grid = $derived(monthGrid(viewY, viewM));

  // Sincroniza el mes visible cada vez que se ABRE (clic propio o auto-open de "Hasta").
  let wasOpen = false;
  $effect(() => {
    if (open && !wasOpen) syncView();
    wasOpen = open;
  });

  // Cerrar al clicar fuera o con Escape.
  $effect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => { if (root && !root.contains(e.target as Node)) open = false; };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') open = false; };
    document.addEventListener('pointerdown', onDown, true);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onDown, true);
      document.removeEventListener('keydown', onKey);
    };
  });
</script>

<div class="df" bind:this={root}>
  <button type="button" class="trigger" class:ph={!value} onclick={toggle} aria-haspopup="dialog" aria-expanded={open}>
    {value ? prettyDate(value) : placeholder}
  </button>
  {#if open}
    <div class="pop" role="dialog" aria-label="Elegir fecha">
      <div class="nav">
        <button type="button" class="arw" onclick={prevMonth} aria-label="Mes anterior">‹</button>
        <span class="mo">{MONTHS_ES[viewM]}</span>
        <button type="button" class="arw" onclick={nextMonth} aria-label="Mes siguiente">›</button>
        <span class="grow"></span>
        <button type="button" class="arw" onclick={prevYear} aria-label="Año anterior">‹</button>
        <span class="yr">{viewY}</span>
        <button type="button" class="arw" onclick={nextYear} aria-label="Año siguiente">›</button>
      </div>
      <div class="wd">{#each WEEKDAYS_ES as w (w)}<span>{w}</span>{/each}</div>
      <div class="days">
        {#each grid as c (c.iso)}
          {@const disabled = c.iso > max}
          <button
            type="button"
            class="day"
            class:out={!c.inMonth}
            class:sel={c.iso === value}
            {disabled}
            onclick={() => choose(c.iso, disabled)}
          >{c.day}</button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .df { position: relative; display: inline-flex; }
  .trigger {
    font: inherit; font-size: 11px; font-variant-numeric: tabular-nums;
    background: var(--cg-surface); color: var(--cg-text-secondary);
    border: 1px solid var(--cg-border); border-radius: 6px; padding: 2px 8px; cursor: pointer;
  }
  .trigger:hover { color: var(--cg-text); border-color: var(--cg-text-tertiary); }
  .trigger.ph { color: var(--cg-text-tertiary); }

  .pop {
    position: absolute; top: calc(100% + 5px); left: 0; z-index: 50;
    width: 236px; padding: 8px; background: var(--cg-surface);
    border: 1px solid var(--cg-border); border-radius: 10px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
  }
  .nav { display: flex; align-items: center; gap: 4px; margin-bottom: 6px; }
  .nav .grow { flex: 1; }
  .arw {
    font: inherit; font-size: 13px; line-height: 1; color: var(--cg-text-secondary);
    background: transparent; border: 1px solid transparent; border-radius: 5px;
    padding: 2px 6px; cursor: pointer;
  }
  .arw:hover { color: var(--cg-text); background: var(--cg-bg); }
  .mo { font-size: 12px; color: var(--cg-text); min-width: 62px; text-align: center; text-transform: capitalize; }
  .yr { font-size: 12px; color: var(--cg-text); font-variant-numeric: tabular-nums; min-width: 34px; text-align: center; }

  .wd { display: grid; grid-template-columns: repeat(7, 1fr); gap: 2px; margin-bottom: 2px; }
  .wd span { font-size: 9px; text-transform: uppercase; letter-spacing: .04em; color: var(--cg-text-tertiary); text-align: center; }

  .days { display: grid; grid-template-columns: repeat(7, 1fr); gap: 2px; }
  .day {
    font: inherit; font-size: 11px; font-variant-numeric: tabular-nums;
    height: 26px; color: var(--cg-text); background: transparent;
    border: 1px solid transparent; border-radius: 6px; cursor: pointer;
  }
  .day:hover:not(:disabled) { background: var(--cg-bg); border-color: var(--cg-border); }
  .day.out { color: var(--cg-text-tertiary); }
  .day.sel { background: var(--cg-accent); color: var(--cg-on-accent); border-color: var(--cg-accent); font-weight: 600; }
  .day:disabled { color: var(--cg-text-tertiary); opacity: 0.35; cursor: default; }
</style>
