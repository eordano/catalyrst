export function truncateAddress(address: string): string {
  return address.length > 12
    ? `${address.slice(0, 6)}\u{2026}${address.slice(-4)}`
    : address;
}

export function hueFor(seed: unknown): number {
  const s = String(seed ?? "");
  let h = 0;
  for (let i = 0; i < s.length; i += 1) h = (h * 31 + s.charCodeAt(i)) % 360;
  return h;
}

const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

export function isValidAddress(addr: string | null | undefined): boolean {
  return ADDRESS_RE.test((addr ?? "").trim());
}
