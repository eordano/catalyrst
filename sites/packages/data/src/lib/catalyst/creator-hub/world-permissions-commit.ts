import { signedFetch } from "../../auth/signer";
import type { AuthIdentity, SignedFetchMetadata } from "../../auth/types";
import { CatalystError, worldsBase } from "../client";
import type { AccessType } from "./world-permissions";

export type CommitWorldAccessInput = {
  accessType: AccessType;
  collaborators: string[];
  secret?: string;
};

export type CommitWorldAccessOptions = {
  identity: AuthIdentity;
  base?: string;
  signal?: AbortSignal;
};

export function accessMetadata(input: CommitWorldAccessInput): SignedFetchMetadata {
  switch (input.accessType) {
    case "unrestricted":
      return { type: "unrestricted" };
    case "shared-secret": {
      const secret = (input.secret ?? "").trim();
      if (!secret) {
        throw new Error("A shared secret is required for shared-secret access.");
      }
      return { type: "shared-secret", secret };
    }
    case "allow-list":
    default:
      return {
        type: "allow-list",
        wallets: input.collaborators.map((w) => w.trim().toLowerCase()).filter(Boolean),
      };
  }
}

export async function commitWorldAccess(
  worldName: string,
  input: CommitWorldAccessInput,
  opts: CommitWorldAccessOptions,
): Promise<void> {
  const name = worldName.trim();
  if (!name) throw new Error("commitWorldAccess: a world name is required.");
  const path = `/world/${encodeURIComponent(name)}/permissions/access`;
  const url = `${worldsBase(opts.base)}${path}`;

  let res: Response;
  try {
    res = await signedFetch(opts.identity, url, {
      method: "POST",
      metadata: accessMetadata(input),
      signal: opts.signal,
    });
  } catch (err) {
    throw new CatalystError(
      `Saving access failed: ${(err as Error)?.message ?? "network error"}`,
      url,
    );
  }

  if (!res.ok) {
    let detail = res.statusText;
    try {
      const body = (await res.json()) as { message?: string; error?: string };
      detail = body?.message ?? body?.error ?? detail;
    } catch {
    }
    throw new CatalystError(`Saving access failed (${res.status}): ${detail}`, url);
  }
}
