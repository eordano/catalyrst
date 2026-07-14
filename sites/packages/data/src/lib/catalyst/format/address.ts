export const ETH_ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

export function isEthAddress(v: string): boolean {
  return ETH_ADDRESS_RE.test(v);
}

export function shortAddress(value: string): string {
  return value.length > 12 ? `${value.slice(0, 6)}\u{2026}${value.slice(-4)}` : value;
}
