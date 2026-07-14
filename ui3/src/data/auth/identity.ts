import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";

export type AuthLinkType = "SIGNER" | "ECDSA_EPHEMERAL" | "ECDSA_SIGNED_ENTITY";

export type AuthLink = {
  type: AuthLinkType;
  payload: string;
  signature: string;
};

export type EphemeralKey = {
  address: string;
  privateKey: `0x${string}`;
};

export type AuthIdentity = {
  signer: string;
  ephemeral: EphemeralKey;
  expiration: string;
  authChain: AuthLink[];
};

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
