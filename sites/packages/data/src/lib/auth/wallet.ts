export type Eip1193Provider = {
  request(args: { method: string; params?: unknown[] }): Promise<unknown>;
  on?(event: string, handler: (...args: unknown[]) => void): void;
  removeListener?(event: string, handler: (...args: unknown[]) => void): void;
};

declare global {
  interface Window {
    ethereum?: Eip1193Provider;
  }
}

export class WalletError extends Error {
  readonly code?: number;
  constructor(message: string, code?: number) {
    super(message);
    this.name = "WalletError";
    this.code = code;
  }
}

export function hasWallet(): boolean {
  if (typeof window === "undefined") return false;
  if (window.ethereum) return true;
  startWalletDiscovery();
  return eip6963Registry.size > 0;
}

export type DetectedWallet = {
  rdns: string;
  name: string;
  icon?: string;
};

type Eip6963Detail = {
  info: { uuid: string; name: string; icon: string; rdns: string };
  provider: Eip1193Provider;
};

const eip6963Registry = new Map<string, Eip6963Detail>();
let eip6963Started = false;

function startWalletDiscovery(): void {
  if (eip6963Started || typeof window === "undefined") return;
  eip6963Started = true;
  window.addEventListener("eip6963:announceProvider", (event: Event) => {
    const detail = (event as CustomEvent<Eip6963Detail>).detail;
    if (detail?.info?.rdns) eip6963Registry.set(detail.info.rdns, detail);
  });
  window.dispatchEvent(new Event("eip6963:requestProvider"));
}

export function detectWallets(): DetectedWallet[] {
  if (typeof window === "undefined") return [];
  startWalletDiscovery();
  const announced = [...eip6963Registry.values()].map((d) => ({
    rdns: d.info.rdns,
    name: d.info.name,
    icon: d.info.icon,
  }));
  if (announced.length > 0) return announced;
  if (window.ethereum) {
    return [{ rdns: "injected", name: legacyWalletName() }];
  }
  return [];
}

function legacyWalletName(): string {
  const e = window.ethereum as
    | (Eip1193Provider & Record<string, boolean>)
    | undefined;
  if (!e) return "Browser wallet";
  if (e.isRabby) return "Rabby";
  if (e.isCoinbaseWallet) return "Coinbase Wallet";
  if (e.isMetaMask) return "MetaMask";
  return "Browser wallet";
}

let activeProvider: Eip1193Provider | null = null;

export function selectWallet(rdns: string | null): void {
  if (!rdns || rdns === "injected") {
    activeProvider = null;
    return;
  }
  activeProvider = eip6963Registry.get(rdns)?.provider ?? null;
}

function provider(): Eip1193Provider {
  const p =
    activeProvider ??
    (typeof window !== "undefined" ? window.ethereum : undefined);
  if (!p) {
    throw new WalletError(
      "No browser wallet found. Install MetaMask (or another EIP-1193 wallet) to continue.",
    );
  }
  return p;
}

export function walletProvider(): Eip1193Provider {
  return provider();
}

function normalize(addr: string): string {
  return addr.trim().toLowerCase();
}

export async function getConnectedAddress(): Promise<string | null> {
  if (!hasWallet()) return null;
  try {
    const accounts = (await provider().request({
      method: "eth_accounts",
    })) as string[];
    return accounts?.[0] ? normalize(accounts[0]) : null;
  } catch {
    return null;
  }
}

export async function connectWallet(): Promise<string> {
  const accounts = (await provider().request({
    method: "eth_requestAccounts",
  })) as string[];
  const addr = accounts?.[0];
  if (!addr) throw new WalletError("Wallet returned no accounts.");
  return normalize(addr);
}

function utf8ToHex(input: string): `0x${string}` {
  const bytes = new TextEncoder().encode(input);
  let hex = "0x";
  for (const b of bytes) hex += b.toString(16).padStart(2, "0");
  return hex as `0x${string}`;
}

export async function personalSign(
  message: string,
  address: string,
): Promise<string> {
  const sig = (await provider().request({
    method: "personal_sign",
    params: [utf8ToHex(message), address],
  })) as string;
  if (typeof sig !== "string" || !sig.startsWith("0x")) {
    throw new WalletError("Wallet returned an invalid signature.");
  }
  return sig;
}

export async function signTypedData(
  typedData: unknown,
  address: string,
): Promise<string> {
  const sig = (await provider().request({
    method: "eth_signTypedData_v4",
    params: [address, JSON.stringify(typedData)],
  })) as string;
  if (typeof sig !== "string" || !sig.startsWith("0x")) {
    throw new WalletError("Wallet returned an invalid typed-data signature.");
  }
  return sig;
}

export async function getChainId(): Promise<number> {
  const hex = (await provider().request({ method: "eth_chainId" })) as string;
  return Number.parseInt(hex, 16);
}

