import { privateKeyToAccount } from "viem/accounts";
import { z } from "zod";

import type { AuthIdentity, AuthLink } from "../../auth/types";
import { catalystBase, worldsBase } from "../client";
import { hashFile, utf8 } from "../hashing";

export type DeployContentFile = { file: string; content: Uint8Array };

export type PreparedDeployment = {
  entityId: string;
  entity: Record<string, unknown>;
  entityFile: Uint8Array;
  content: { file: string; hash: string }[];
  files: { file: string; hash: string; content: Uint8Array }[];
};

export type BuildSceneEntityInput = {
  pointers: string[];
  files: DeployContentFile[];
  metadata: unknown;
  timestamp?: number;
  referencedHashes?: { file: string; hash: string }[];
};

const HASH_RE = /^(bafk|bafy)[a-z2-7]+$/;

export async function buildSceneEntity(
  input: BuildSceneEntityInput,
): Promise<PreparedDeployment> {
  if (input.pointers.length === 0) {
    throw new Error("buildSceneEntity: at least one pointer is required");
  }

  const seenNames = new Set<string>();
  const assertUnique = (file: string) => {
    const lower = file.toLowerCase();
    if (seenNames.has(lower)) {
      throw new Error(`buildSceneEntity: duplicate (case-insensitive) file "${file}"`);
    }
    seenNames.add(lower);
  };

  const files: PreparedDeployment["files"] = [];
  const content: { file: string; hash: string }[] = [];
  for (const f of input.files) {
    assertUnique(f.file);
    const hash = await hashFile(f.content);
    files.push({ file: f.file, hash, content: f.content });
    content.push({ file: f.file, hash });
  }
  for (const r of input.referencedHashes ?? []) {
    assertUnique(r.file);
    if (!HASH_RE.test(r.hash)) {
      throw new Error(`buildSceneEntity: referenced hash for "${r.file}" is not a CIDv1`);
    }
    content.push({ file: r.file, hash: r.hash });
  }

  const entityNoId = {
    version: "v3" as const,
    type: "scene" as const,
    pointers: input.pointers,
    timestamp: input.timestamp ?? Date.now(),
    content,
    metadata: input.metadata,
  };
  const entityFile = utf8(JSON.stringify(entityNoId));
  const entityId = await hashFile(entityFile);

  return {
    entityId,
    entity: { id: entityId, ...entityNoId },
    entityFile,
    content,
    files,
  };
}

export async function buildDeployAuthChain(
  identity: AuthIdentity,
  entityId: string,
): Promise<AuthLink[]> {
  const ephemeral = privateKeyToAccount(identity.ephemeral.privateKey);
  const signature = await ephemeral.signMessage({ message: entityId });
  return [
    ...identity.authChain,
    { type: "ECDSA_SIGNED_ENTITY", payload: entityId, signature },
  ];
}

export async function buildSimpleDeployAuthChain(
  signerPrivateKey: `0x${string}`,
  entityId: string,
): Promise<AuthLink[]> {
  const account = privateKeyToAccount(signerPrivateKey);
  const signature = await account.signMessage({ message: entityId });
  return [
    { type: "SIGNER", payload: account.address.toLowerCase(), signature: "" },
    { type: "ECDSA_SIGNED_ENTITY", payload: entityId, signature },
  ];
}

export type DeployResult =
  | { ok: true; status: number; creationTimestamp: number | null }
  | { ok: false; status: number; errors: string[] };

const DeployOkBodySchema = z
  .object({ creationTimestamp: z.number().nullish() })
  .nullable();

const DeployErrorBodySchema = z.object({
  errors: z.array(z.unknown()).nullish(),
  error: z.string().nullish(),
  message: z.string().nullish(),
});

export type DeployOptions = {
  base?: string;
  path?: string;
  fetchImpl?: typeof fetch;
  signal?: AbortSignal;
};

export const CATALYST_DEPLOY_PATH = "/content/entities";
export const WORLDS_DEPLOY_PATH = "/entities";

export function buildDeployFormData(
  prepared: PreparedDeployment,
  authChain: AuthLink[],
): FormData {
  const form = new FormData();
  form.append("entityId", prepared.entityId);
  authChain.forEach((link, i) => {
    form.append(`authChain[${i}][type]`, link.type);
    form.append(`authChain[${i}][payload]`, link.payload);
    form.append(`authChain[${i}][signature]`, link.signature ?? "");
  });
  form.append(prepared.entityId, new Blob([toArrayBuffer(prepared.entityFile)]), prepared.entityId);
  for (const f of prepared.files) {
    form.append(f.hash, new Blob([toArrayBuffer(f.content)]), f.hash);
  }
  return form;
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const out = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(out).set(bytes);
  return out;
}

export async function deployScene(
  identity: AuthIdentity,
  prepared: PreparedDeployment,
  opts: DeployOptions = {},
): Promise<DeployResult> {
  const authChain = await buildDeployAuthChain(identity, prepared.entityId);
  return postDeployment(prepared, authChain, opts);
}

export async function postDeployment(
  prepared: PreparedDeployment,
  authChain: AuthLink[],
  opts: DeployOptions = {},
): Promise<DeployResult> {
  const base = catalystBase(opts.base);
  const path = opts.path ?? CATALYST_DEPLOY_PATH;
  const doFetch = opts.fetchImpl ?? fetch;
  const form = buildDeployFormData(prepared, authChain);

  let res: Response;
  try {
    res = await doFetch(`${base}${path}`, {
      method: "POST",
      body: form,
      signal: opts.signal,
    });
  } catch (err) {
    return { ok: false, status: 0, errors: [(err as Error)?.message ?? "network error"] };
  }

  let body: unknown = null;
  try {
    body = await res.json();
  } catch {
    body = null;
  }

  if (res.ok) {
    const parsed = DeployOkBodySchema.safeParse(body);
    if (!parsed.success) {
      return {
        ok: false,
        status: res.status,
        errors: [
          `deploy response failed schema validation: ${parsed.error.issues
            .slice(0, 5)
            .map((i) => `${i.path.map(String).join(".")}:${i.code}`)
            .join(", ")}`,
        ],
      };
    }
    return {
      ok: true,
      status: res.status,
      creationTimestamp: parsed.data?.creationTimestamp ?? null,
    };
  }
  return { ok: false, status: res.status, errors: extractErrors(body, res.statusText) };
}

export type WorldSceneMetadata = {
  scene: { base?: string; parcels: string[]; [k: string]: unknown };
  worldConfiguration?: { name?: string; [k: string]: unknown };
  [k: string]: unknown;
};

export type DeployWorldSceneInput = {
  worldName: string;
  files: DeployContentFile[];
  metadata: WorldSceneMetadata;
  timestamp?: number;
  referencedHashes?: { file: string; hash: string }[];
};

export async function deployWorldScene(
  identity: AuthIdentity,
  input: DeployWorldSceneInput,
  opts: DeployOptions = {},
): Promise<DeployResult> {
  const worldName = input.worldName?.trim();
  if (!worldName) {
    throw new Error(
      "deployWorldScene: a worldConfiguration.name (target World name) is required",
    );
  }
  const parcels = input.metadata?.scene?.parcels ?? [];
  if (parcels.length === 0) {
    throw new Error(
      "deployWorldScene: metadata.scene.parcels is required (pointers derive from it)",
    );
  }

  const metadata: WorldSceneMetadata = {
    ...input.metadata,
    worldConfiguration: { ...input.metadata.worldConfiguration, name: worldName },
  };

  const prepared = await buildSceneEntity({
    pointers: parcels,
    files: input.files,
    metadata,
    timestamp: input.timestamp,
    referencedHashes: input.referencedHashes,
  });

  return deployScene(identity, prepared, {
    ...opts,
    base: opts.base ?? worldsBase(),
    path: opts.path ?? WORLDS_DEPLOY_PATH,
  });
}

export type DeployLandSceneInput = {
  files: DeployContentFile[];
  metadata: WorldSceneMetadata;
  timestamp?: number;
  referencedHashes?: { file: string; hash: string }[];
};

export async function deployLandScene(
  identity: AuthIdentity,
  input: DeployLandSceneInput,
  opts: DeployOptions = {},
): Promise<DeployResult> {
  const parcels = input.metadata?.scene?.parcels ?? [];
  if (parcels.length === 0) {
    throw new Error(
      "deployLandScene: metadata.scene.parcels is required (pointers derive from it)",
    );
  }

  const metadata: WorldSceneMetadata = { ...input.metadata };
  delete metadata.worldConfiguration;

  const prepared = await buildSceneEntity({
    pointers: parcels,
    files: input.files,
    metadata,
    timestamp: input.timestamp,
    referencedHashes: input.referencedHashes,
  });

  return deployScene(identity, prepared, opts);
}

function extractErrors(body: unknown, fallback: string): string[] {
  const parsed = DeployErrorBodySchema.safeParse(body);
  if (parsed.success) {
    const { errors, error, message } = parsed.data;
    if (Array.isArray(errors) && errors.length > 0) return errors.map((e) => String(e));
    if (error) return [error];
    if (message) return [message];
  }
  return [fallback || "deploy failed"];
}
