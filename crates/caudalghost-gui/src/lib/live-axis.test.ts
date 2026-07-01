import { expect, test } from 'vitest';
import { smoothNow, liveTicks, liveLabel } from './live-axis';

test('smoothNow avanza con el tiempo y se congela en lead', () => {
  expect(smoothNow(1000, 0.5, 1.25)).toBe(1000.5);   // dentro del lead
  expect(smoothNow(1000, 3, 1.25)).toBe(1001.25);    // se congela en latest+lead
  expect(smoothNow(1000, -1, 1.25)).toBe(1000);      // elapsed negativo → latest
});

test('liveTicks: offsets fijos desde max, ascendente, dentro de [min,max]', () => {
  expect(liveTicks(40, 100, 15)).toEqual([40, 55, 70, 85, 100]);
  expect(liveTicks(100, 100, 15)).toEqual([100]);
});

test('liveLabel: relativo al ahora (max)', () => {
  expect(liveLabel(100, 100)).toBe('ahora');
  expect(liveLabel(85, 100)).toBe('15s');
  expect(liveLabel(40, 100)).toBe('1 min');   // 60s
  expect(liveLabel(10, 100)).toBe('1 min');   // 90s → floor
  expect(liveLabel(41, 100)).toBe('59s');    // último antes del corte a minutos
  expect(liveLabel(105, 100)).toBe('ahora'); // t > max (ago negativo) → ahora
});
