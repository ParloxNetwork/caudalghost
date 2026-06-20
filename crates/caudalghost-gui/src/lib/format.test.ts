import { expect, test } from 'vitest';
import { formatUnits } from './format';

test('binary KiB', () => { expect(formatUnits(1536, 'binary')).toBe('1.5 KiB'); });
test('decimal KB', () => { expect(formatUnits(1500, 'decimal')).toBe('1.5 KB'); });
test('bits Kb (x8)', () => { expect(formatUnits(1000, 'bits')).toBe('8.0 Kb'); });
test('bytes pequeños', () => { expect(formatUnits(512, 'binary')).toBe('512.0 B'); });
