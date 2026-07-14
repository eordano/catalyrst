import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";

import type { AuthIdentity, AuthLink, EphemeralKey } from "./types";
import { connectWallet, getConnectedAddress, personalSign } from "./wallet";

export const DEFAULT_EXPIRATION_MS = 7 * 24 * 60 * 60 * 1000;

export function generateEphemeralKey(): EphemeralKey {
  const privateKey = generatePrivateKey();
  const account = privateKeyToAccount(privateKey);
  return { address: account.address.toLowerCase(), privateKey };
}

export function buildEphemeralMessage(
  ephemeralAddress: string,
  expiration: Date,
): string {
  return [
    "Decentraland Login",
    `Ephemeral address: ${ephemeralAddress}`,
    `Expiration: ${expiration.toISOString()}`,
  ].join("\n");
}

export async function createIdentityWith(
  signer: string,
  signMessage: (message: string) => Promise<string>,
  opts: { expirationMs?: number } = {},
): Promise<AuthIdentity> {
  const ephemeral = generateEphemeralKey();
  const expiration = new Date(
    Date.now() + (opts.expirationMs ?? DEFAULT_EXPIRATION_MS),
  );
  const message = buildEphemeralMessage(ephemeral.address, expiration);
  const signature = await signMessage(message);

  const authChain: AuthLink[] = [
    { type: "SIGNER", payload: signer, signature: "" },
    { type: "ECDSA_EPHEMERAL", payload: message, signature },
  ];

  return {
    signer: signer.toLowerCase(),
    ephemeral,
    expiration: expiration.toISOString(),
    authChain,
  };
}

export function createIdentityFor(
  signer: string,
  opts: { expirationMs?: number } = {},
): Promise<AuthIdentity> {
  return createIdentityWith(
    signer,
    (message) => personalSign(message, signer),
    opts,
  );
}

export async function createIdentity(
  opts: { expirationMs?: number } = {},
): Promise<AuthIdentity> {
  const signer = (await getConnectedAddress()) ?? (await connectWallet());
  return createIdentityFor(signer, opts);
}

export async function createIdentityFromPrivateKey(
  signerPrivateKey: `0x${string}`,
  opts: { expirationMs?: number } = {},
): Promise<AuthIdentity> {
  const account = privateKeyToAccount(signerPrivateKey);
  const signer = account.address.toLowerCase();
  const ephemeral = generateEphemeralKey();
  const expiration = new Date(
    Date.now() + (opts.expirationMs ?? DEFAULT_EXPIRATION_MS),
  );
  const message = buildEphemeralMessage(ephemeral.address, expiration);
  const signature = await account.signMessage({ message });

  const authChain: AuthLink[] = [
    { type: "SIGNER", payload: signer, signature: "" },
    { type: "ECDSA_EPHEMERAL", payload: message, signature },
  ];

  return { signer, ephemeral, expiration: expiration.toISOString(), authChain };
}

export { isIdentityExpired } from "./expiry";
