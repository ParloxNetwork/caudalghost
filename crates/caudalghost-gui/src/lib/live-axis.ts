// Lógica pura del eje del gráfico EN VIVO (fluido). Sin dependencias de uPlot.

/** Reloj suave para el scroll: avanza con el tiempo transcurrido desde la última muestra,
 *  pero como mucho `lead` s (así se congela si el daemon deja de emitir: pausa / caída). */
export function smoothNow(latestTs: number, elapsedSec: number, lead: number): number {
  return latestTs + Math.min(Math.max(elapsedSec, 0), lead);
}

/** Posiciones de tick en offsets fijos desde `max` ("ahora"), ascendente, dentro de [min,max]. */
export function liveTicks(min: number, max: number, stepSec = 15): number[] {
  const out: number[] = [];
  for (let t = max; t >= min; t -= stepSec) out.push(t);
  return out.reverse();
}

/** Etiqueta relativa al ahora (`max`): "ahora" / "Ns" / "N min". */
export function liveLabel(t: number, max: number): string {
  const ago = Math.round(max - t);
  if (ago <= 0) return 'ahora';
  if (ago < 60) return `${ago}s`;
  return `${Math.floor(ago / 60)} min`;
}
