import { expect, test } from 'vitest';
import { clampTooltip } from './chart-geom';

test('primer punto (left≈0): el centro se recorta a medio ancho', () => {
  const r = clampTooltip(0, 100, 120, 30, 600, 240);
  expect(r.left).toBe(60); // no deja que el borde izquierdo salga
  expect(r.flipY).toBe(false);
});

test('último punto (left≈plotW): el centro se recorta a plotW - medio ancho', () => {
  const r = clampTooltip(600, 100, 120, 30, 600, 240);
  expect(r.left).toBe(540);
});

test('punto en zona central: sin recorte', () => {
  const r = clampTooltip(300, 100, 120, 30, 600, 240);
  expect(r.left).toBe(300);
});

test('punto muy arriba: flip a debajo', () => {
  const r = clampTooltip(300, 5, 120, 30, 600, 240); // 5 - 30 - 8 < 0
  expect(r.flipY).toBe(true);
});
