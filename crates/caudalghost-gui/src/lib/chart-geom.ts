// Geometría pura del tooltip: recorta el centro X para que la caja no se salga
// del plot, y decide si debe ir debajo del punto cuando no cabe arriba.
const GAP = 8;

export function clampTooltip(
  left: number, top: number, boxW: number, boxH: number, plotW: number, plotH: number,
): { left: number; flipY: boolean } {
  const half = boxW / 2;
  const clampedLeft = Math.max(half, Math.min(plotW - half, left));
  const flipY = top - boxH - GAP < 0; // no cabe arriba → va debajo
  return { left: clampedLeft, flipY };
}
