import { getJSON, worldsBase, CatalystError } from "../client";
import type { GetOptions } from "../client";
import { signedFetch } from "../../auth/signer";
import type { AuthIdentity } from "../../auth/types";

export type ParcelsPermission = { parcels: string[]; total: number };

/**
 * null when the parcel allow-list could not be read.
 *
 * `{ parcels: [], total: 0 }` is a real answer with a specific meaning: the
 * wallet holds the permission world-wide rather than per parcel, which
 * `creator-hub.world-settings` reads as "may unpublish every scene". A failed
 * read used to arrive as exactly that, handing a collaborator the widest reading
 * of a permission nobody looked up.
 */
export async function fetchParcelsPermission(
  worldName: string,
  address: string,
  opts: GetOptions & { permission?: string } = {},
): Promise<ParcelsPermission | null> {
  const permission = opts.permission ?? "deployment";
  const path = `/world/${encodeURIComponent(worldName)}/permissions/${encodeURIComponent(
    permission,
  )}/address/${encodeURIComponent(address.toLowerCase())}/parcels`;
  try {
    const res = await getJSON<{ parcels?: string[]; total?: number }>(path, opts);
    return { parcels: res.parcels ?? [], total: res.total ?? 0 };
  } catch {
    return null;
  }
}

export type UnpublishOptions = {
  identity: AuthIdentity;
  base?: string;
  signal?: AbortSignal;
};

export async function unpublishWorldScene(
  worldName: string,
  sceneCoord: string,
  opts: UnpublishOptions,
): Promise<void> {
  const path = `/world/${encodeURIComponent(worldName)}/scenes/${encodeURIComponent(sceneCoord)}`;
  const url = `${worldsBase(opts.base)}${path}`;
  let res: Response;
  try {
    res = await signedFetch(opts.identity, url, { method: "DELETE", signal: opts.signal });
  } catch (err) {
    throw new CatalystError(
      `Unpublish failed: ${(err as Error)?.message ?? "network error"}`,
      url,
    );
  }
  if (!res.ok) {
    let detail = "";
    try {
      detail = (await res.text()).slice(0, 300);
    } catch {
    }
    throw new CatalystError(
      `Unpublish failed (${res.status})${detail ? `: ${detail}` : ""}`,
      url,
    );
  }
}
