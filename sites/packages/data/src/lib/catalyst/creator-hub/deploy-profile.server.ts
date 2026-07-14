import { createHash } from "node:crypto";

import { z } from "zod";

import { catalystBase } from "../client";

export type AuthChainDelegate = {
  type: string;
  payload: string;
  signature: string;
};

export type AuthIdentity = {
  signerAddress: string;
  signMessage: (message: string) => Promise<string>;
  delegates?: AuthChainDelegate[];
};

export type AvatarColor = { color: { r: number; g: number; b: number; a?: number } };

export type AvatarWireFormat = {
  bodyShape?: string;
  eyes?: AvatarColor;
  hair?: AvatarColor;
  skin?: AvatarColor;
  wearables: string[];
  forceRender?: string[];
  emotes?: { slot: number; urn: string }[];
};

export type ProfileAvatar = {
  name: string;
  version: number;
  ethAddress: string;
  hasClaimedName: boolean;
  avatar: AvatarWireFormat;
  userId?: string;
  hasConnectedWeb3?: boolean;
  [key: string]: unknown;
};

export type ContentFile = { file: string; hash: string };

export type ProfileDeployment = {
  version: "v3";
  type: "profile";
  pointers: string[];
  timestamp: number;
  content: ContentFile[];
  metadata: { avatars: ProfileAvatar[] };
};

const RFC4648_BASE32_LOWER = "abcdefghijklmnopqrstuvwxyz234567";

function multibaseBase32Lower(data: Uint8Array): string {
  let bits = 0;
  let value = 0;
  let out = "";
  for (let i = 0; i < data.length; i++) {
    value = (value << 8) | data[i];
    bits += 8;
    while (bits >= 5) {
      out += RFC4648_BASE32_LOWER[(value >>> (bits - 5)) & 31];
      bits -= 5;
    }
  }
  if (bits > 0) {
    out += RFC4648_BASE32_LOWER[(value << (5 - bits)) & 31];
  }
  return `b${out}`;
}

function pushVarint(buf: number[], v: number): void {
  let value = v;
  for (;;) {
    const byte = value & 0x7f;
    value >>>= 7;
    if (value === 0) {
      buf.push(byte);
      break;
    }
    buf.push(byte | 0x80);
  }
}

export function computeEntityId(bytes: Uint8Array): string {
  const digest = createHash("sha256").update(bytes).digest();
  const cidBytes: number[] = [];
  pushVarint(cidBytes, 1);
  pushVarint(cidBytes, 0x55);
  cidBytes.push(0x12);
  cidBytes.push(0x20);
  for (const b of digest) cidBytes.push(b);
  return multibaseBase32Lower(Uint8Array.from(cidBytes));
}

export type AuthChainLink = { type: string; payload: string; signature: string };

export function buildAuthChain(
  signerAddress: string,
  entityId: string,
  signature: string,
  delegates: AuthChainDelegate[] = [],
): AuthChainLink[] {
  return [
    { type: "SIGNER", payload: signerAddress, signature: "" },
    ...delegates.map((d) => ({ ...d })),
    { type: "ECDSA_SIGNED_ENTITY", payload: entityId, signature },
  ];
}

export type ProfileEntityInput = {
  address: string;
  name?: string;
  version?: number;
  hasClaimedName?: boolean;
  avatar: AvatarWireFormat;
  extra?: Record<string, unknown>;
  timestamp?: number;
};

export const DEFAULT_PROFILE_EMOTES: { slot: number; urn: string }[] = [
  { slot: 0, urn: "handsair" },
  { slot: 1, urn: "wave" },
  { slot: 2, urn: "fistpump" },
  { slot: 3, urn: "dance" },
  { slot: 4, urn: "raisehand" },
  { slot: 5, urn: "clap" },
  { slot: 6, urn: "money" },
  { slot: 7, urn: "kiss" },
  { slot: 8, urn: "headexplode" },
  { slot: 9, urn: "shrug" },
];

export function buildProfileDeployment(input: ProfileEntityInput): ProfileDeployment {
  const address = input.address.trim().toLowerCase();
  const timestamp = input.timestamp ?? Date.now();
  const avatarEntry: ProfileAvatar = {
    name: input.name ?? "",
    version: input.version ?? 1,
    ethAddress: address,
    hasClaimedName: input.hasClaimedName ?? false,
    avatar: {
      ...input.avatar,
      emotes: input.avatar.emotes ?? DEFAULT_PROFILE_EMOTES,
    },
    ...(input.extra ?? {}),
  };
  return {
    version: "v3",
    type: "profile",
    pointers: [address],
    timestamp,
    content: [],
    metadata: { avatars: [avatarEntry] },
  };
}

export type DeployProfileOptions = {
  base?: string;
  fetchImpl?: typeof fetch;
  signal?: AbortSignal;
};

const DeployAckSchema = z
  .object({ creationTimestamp: z.number().nullish() })
  .nullable();
export type DeployAck = z.infer<typeof DeployAckSchema>;

export type DeployProfileResult = {
  entityId: string;
  response: DeployAck;
};

export async function deployProfile(
  input: ProfileEntityInput,
  identity: AuthIdentity,
  opts: DeployProfileOptions = {},
): Promise<DeployProfileResult> {
  const deployment = buildProfileDeployment(input);
  const entityJson = JSON.stringify(deployment);
  const entityBytes = new TextEncoder().encode(entityJson);

  const entityId = computeEntityId(entityBytes);

  const signature = await identity.signMessage(entityId);
  const authChain = buildAuthChain(
    identity.signerAddress,
    entityId,
    signature,
    identity.delegates ?? [],
  );

  const form = new FormData();
  form.set("entityId", entityId);
  authChain.forEach((link, i) => {
    form.set(`authChain[${i}][type]`, link.type);
    form.set(`authChain[${i}][payload]`, link.payload);
    form.set(`authChain[${i}][signature]`, link.signature);
  });
  form.set(
    entityId,
    new Blob([entityBytes], { type: "application/octet-stream" }),
    entityId,
  );

  const base = catalystBase(opts.base);
  const url = `${base}/content/entities`;
  const doFetch = opts.fetchImpl ?? fetch;

  const res = await doFetch(url, {
    method: "POST",
    body: form,
    signal: opts.signal,
  });

  if (!res.ok) {
    let detail = "";
    try {
      detail = await res.text();
    } catch {
    }
    throw new Error(
      `profile deploy failed: ${res.status} ${res.statusText}${detail ? `: ${detail}` : ""}`,
    );
  }

  let raw: unknown = null;
  try {
    raw = await res.json();
  } catch {
    raw = null;
  }
  const ack = DeployAckSchema.safeParse(raw);
  if (!ack.success) {
    throw new Error(
      `profile deploy returned an unexpected response shape: ${ack.error.issues
        .slice(0, 5)
        .map((i) => `${i.path.map(String).join(".")}:${i.code}`)
        .join(", ")}`,
    );
  }
  return { entityId, response: ack.data };
}

export type ServerDeploySigner = AuthIdentity & {
  isGuest?: boolean;
};

declare global {
  // eslint-disable-next-line no-var
  var __dclServerDeploySigner: ServerDeploySigner | undefined;
}

export function resolveServerSigner(): AuthIdentity | null {
  const s = globalThis.__dclServerDeploySigner;
  if (!s || s.isGuest || !s.signerAddress || typeof s.signMessage !== "function") {
    return null;
  }
  return { signerAddress: s.signerAddress, signMessage: s.signMessage, delegates: s.delegates };
}

export type GatedDeployResult =
  | { deployed: true; entityId: string; response: DeployAck }
  | { deployed: false; reason: "no-signer"; entityId?: undefined };

export async function deployProfileWithSigner(
  input: ProfileEntityInput,
  opts: DeployProfileOptions = {},
): Promise<GatedDeployResult> {
  const signer = resolveServerSigner();
  if (!signer) return { deployed: false, reason: "no-signer" };
  const { entityId, response } = await deployProfile(input, signer, opts);
  return { deployed: true, entityId, response };
}
