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

export type SignedFetchMetadata = Record<string, unknown>;
