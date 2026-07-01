import { expect, test } from 'vitest';
import { chooseTier, fmtFine, fmtCoarse, MONTHS_ES } from './axis-time';

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
