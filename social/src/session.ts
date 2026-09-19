import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { toHex, verifyMessage, type Hex } from "viem";
import type { WalletIdentity } from "./api";

const STORAGE_KEY = "dcl.social.wallet-session.v1";
const DURATION = 24 * 60 * 60 * 1000;
type Session = { address: Hex; key: Hex; expiresAt: number; signature: Hex };
let cached: Session | null = null;
const delegation = (s: Session) =>
  `Decentraland Login\nEphemeral address: ${privateKeyToAccount(s.key).address}\nExpiration: ${new Date(s.expiresAt).toISOString()}`;
function clear() {
  cached = null;
  try { localStorage.removeItem(STORAGE_KEY); sessionStorage.removeItem(STORAGE_KEY); } catch { /* Memory-only fallback. */ }
}
export function forgetWalletSession() { clear(); window.dclSocialIdentity = undefined; window.dispatchEvent(new Event("dcl:identity-changed")); }
export async function walletSession(connect: boolean): Promise<WalletIdentity | null> {
  const provider = window.ethereum;
  if (!provider) {
    if (connect) throw new Error("Open this site in your wallet browser or enable a browser wallet extension.");
    return null;
  }
  const accounts = await provider.request({ method: connect ? "eth_requestAccounts" : "eth_accounts" }) as Hex[];
  const address = accounts[0];
  if (!address) {
    if (connect) throw new Error("No wallet account was selected. Try connecting again.");
    return null;
  }
  let session = cached;
  if (!session) {
    try { session = JSON.parse(localStorage.getItem(STORAGE_KEY) || sessionStorage.getItem(STORAGE_KEY) || "null"); } catch { clear(); }
  }
  try {
    if (!session || session.address.toLowerCase() !== address.toLowerCase() ||
      !Number.isFinite(session.expiresAt) || session.expiresAt <= Date.now() ||
      !await verifyMessage({ address, message: delegation(session), signature: session.signature })) {
      session = null;
      clear();
    }
  } catch { session = null; clear(); }
  if (!session) {
    // Wallet prompts must only follow an explicit Connect action.
    if (!connect) return null;
    session = { address, key: generatePrivateKey(), expiresAt: Date.now() + DURATION, signature: "0x" };
    session.signature = await provider.request({ method: "personal_sign", params: [toHex(delegation(session)), address] }) as Hex;
    const current = await provider.request({ method: "eth_accounts" }) as string[];
    if (current[0]?.toLowerCase() !== address.toLowerCase() ||
      !await verifyMessage({ address, message: delegation(session), signature: session.signature })) {
      clear();
      throw new Error("Wallet changed while authorizing. Connect again.");
    }

  }
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(session)); sessionStorage.removeItem(STORAGE_KEY); } catch { /* Keep the session in memory. */ }
  cached = session;
  const authorized = session;
  const signer = privateKeyToAccount(authorized.key);
  return {
    address,
    expiresAt: authorized.expiresAt,
    get canSignSilently() { return Date.now() < authorized.expiresAt; },
    async signRequest(request) {
      if (Date.now() >= authorized.expiresAt) throw new Error("Session expired. Reconnect your wallet.");
      const current = await provider.request({ method: "eth_accounts" }) as string[];
      if (current[0]?.toLowerCase() !== address.toLowerCase()) {
        clear();
        window.dispatchEvent(new Event("dcl:identity-changed"));
        throw new Error("Wallet changed. Connect again.");
      }
      return [
        { type: "SIGNER", payload: address, signature: "" },
        { type: "ECDSA_EPHEMERAL", payload: delegation(authorized), signature: authorized.signature },
        { type: "ECDSA_SIGNED_ENTITY", payload: request.payload, signature: await signer.signMessage({ message: request.payload }) },
      ];
    },
  };
}
