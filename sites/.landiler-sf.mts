import type { Hex } from "viem";
import { privateKeyToAccount } from "viem/accounts";

const PK = process.env.TEST_PK as Hex | undefined;
const BASE = process.env.BASE;
if (!PK || !BASE) throw new Error("TEST_PK and BASE env vars are required");
const acct = privateKeyToAccount(PK);
const ADDR = acct.address.toLowerCase();
const sp = (u: string): string => (u.startsWith("/") ? u.split("?")[0] : new URL(u).pathname);
const m = process.argv[3];
const p = process.argv[4];
const b: unknown = process.argv[5] ? JSON.parse(process.argv[5]) : undefined;
if (!m || !p) throw new Error("usage: .landiler-sf.mts <method> <path> [jsonBody]");
const ts = Date.now().toString();
const meta = "{}";
const payload = `${m}:${sp(p)}:${ts}:${meta}`.toLowerCase();
const sig = await acct.signMessage({ message: payload });
const h: Record<string, string> = {
  "x-identity-timestamp": ts,
  "x-identity-metadata": meta,
  "content-type": "application/json",
};
if (process.env.IDEM) h["idempotency-key"] = process.env.IDEM;
type AuthChainLink = { type: string; payload: string; signature: string };
const chain: AuthChainLink[] = [
  { type: "SIGNER", payload: ADDR, signature: "" },
  { type: "ECDSA_SIGNED_ENTITY", payload, signature: sig },
];
chain.forEach((l, i) => (h[`x-identity-auth-chain-${i}`] = JSON.stringify(l)));
const r = await fetch(BASE + p, { method: m, headers: h, body: b ? JSON.stringify(b) : undefined });
console.log(JSON.stringify({ status: r.status, body: await r.text() }));
