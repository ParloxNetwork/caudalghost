export type UnitMode = 'binary' | 'decimal' | 'bits';
const U = {
  binary: { base: 1024, names: ['B', 'KiB', 'MiB', 'GiB', 'TiB'] },
  decimal: { base: 1000, names: ['B', 'KB', 'MB', 'GB', 'TB'] },
  bits: { base: 1000, names: ['b', 'Kb', 'Mb', 'Gb', 'Tb'] },
} as const;

export function formatUnits(bytes: number, mode: UnitMode = 'binary'): string {
  const { base, names } = U[mode];
  let v = mode === 'bits' ? bytes * 8 : bytes;
  let i = 0;
  while (v >= base && i < names.length - 1) { v /= base; i++; }
  return `${v.toFixed(1)} ${names[i]}`;
}

export function unitLabel(m: UnitMode): string {
  return m === 'binary' ? 'KiB' : m === 'decimal' ? 'KB' : 'Mbps';
}
