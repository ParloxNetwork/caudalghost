import { expect, test } from 'vitest';
import { chooseTier, fmtFine, fmtCoarse, MONTHS_ES } from './axis-time';
import { coarseUnit, coarseBuckets, coarseMidpoints } from './axis-time';

const DAY = 86400;
test('chooseTier por franjas de span', () => {
  expect(chooseTier(2 * 3600)).toBe('intraday');       // 5m/1h/24h
  expect(chooseTier(2 * DAY)).toBe('intraday');         // borde inclusivo
  expect(chooseTier(7 * DAY)).toBe('days');             // 7d
  expect(chooseTier(120 * DAY)).toBe('days');           // borde inclusivo
  expect(chooseTier(365 * DAY)).toBe('months');         // 1 año
});

test('MONTHS_ES son 12 abreviaturas en español', () => {
  expect(MONTHS_ES).toHaveLength(12);
  expect(MONTHS_ES[5]).toBe('jun');
});

test('fmtFine formatea según el tier', () => {
  const t = new Date(2026, 5, 30, 18, 5).getTime() / 1000; // 30 jun 18:05 local
  expect(fmtFine(t, 'intraday')).toBe('18:05');
  expect(fmtFine(t, 'days')).toBe('30');
  expect(fmtFine(t, 'months')).toBe('jun');
});

test('fmtCoarse formatea según el tier', () => {
  const t = new Date(2026, 5, 30, 18, 5).getTime() / 1000;
  expect(fmtCoarse(t, 'intraday')).toBe('30 jun');
  expect(fmtCoarse(t, 'days')).toBe('jun');
  expect(fmtCoarse(t, 'months')).toBe('2026');
});

test('coarseUnit por tier', () => {
  expect(coarseUnit('intraday')).toBe('day');
  expect(coarseUnit('days')).toBe('month');
  expect(coarseUnit('months')).toBe('year');
});

test('coarseBuckets: 24h que cruza medianoche → 2 días', () => {
  const start = new Date(2026, 5, 30, 12, 0).getTime() / 1000; // 30 jun 12:00
  const end = new Date(2026, 6, 1, 12, 0).getTime() / 1000;    // 1 jul 12:00
  const b = coarseBuckets(start, end, 'day');
  expect(b).toHaveLength(2);
  expect(b[0]).toBe(new Date(2026, 5, 30, 0, 0).getTime() / 1000);
  expect(b[1]).toBe(new Date(2026, 6, 1, 0, 0).getTime() / 1000);
});

test('coarseMidpoints: 5 min dentro de un día → 1 punto centrado en la ventana', () => {
  const start = new Date(2026, 5, 30, 13, 55).getTime() / 1000;
  const end = new Date(2026, 5, 30, 14, 0).getTime() / 1000;
  const m = coarseMidpoints(start, end, 'intraday');
  expect(m).toHaveLength(1);
  expect(m[0]).toBe((start + end) / 2);
});

test('coarseMidpoints: 24h cruzando medianoche → 2 puntos, recortados a la ventana', () => {
  const start = new Date(2026, 5, 30, 12, 0).getTime() / 1000;
  const end = new Date(2026, 6, 1, 12, 0).getTime() / 1000;
  const m = coarseMidpoints(start, end, 'intraday');
  expect(m).toHaveLength(2);
  // bucket 0: [30 12:00, 1 00:00) → medio 30 18:00 ; bucket 1: [1 00:00, 1 12:00) → medio 1 06:00
  expect(m[0]).toBe(new Date(2026, 5, 30, 18, 0).getTime() / 1000);
  expect(m[1]).toBe(new Date(2026, 6, 1, 6, 0).getTime() / 1000);
});
