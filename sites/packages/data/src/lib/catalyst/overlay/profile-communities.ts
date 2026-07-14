import { signedFetch } from "../../auth/signer";
import type { AuthIdentity } from "../../auth/types";

import { CatalystError, catalystBase } from "../client";
import { CommunitySchema } from "./communities";

const PATH = "/v1/communities";

export type ProfileCommunity = {
  id: string;
  name: string;
  /** null when the API did not report a size */
  membersCount: number | null;
  role: "owner" | "admin" | "member";
  thumb: string | null;
};

function unwrapData<T = unknown>(env: unknown): T {
  const e = env as { data?: T } | null;
  return (e?.data ?? env) as T;
}

/** null when the response carried no role. Every row here comes from
 *  `?onlyMemberOf=true` on a signed request, so plain membership is the floor
 *  this list already established -- it is not a rank read off a missing key. */
function mapRole(role: string | null): ProfileCommunity["role"] {
  if (role === "owner") return "owner";
  if (role === "moderator" || role === "admin") return "admin";
  return "member";
}

function mapThumb(thumbnailUrl: string | null): string | null {
  const u = (thumbnailUrl ?? "").trim();
  if (!u || u === "N/A") return null;
  return `url("${u.replace(/"/g, "%22")}") center / cover no-repeat`;
}

function displayableName(name: string | null): boolean {
  if (name === null) return false;
  const n = name.trim();
  return n.length > 0 && n.length <= 60 && !/[<>]/.test(n);
}

export async function fetchMyCommunities(
  identity: AuthIdentity,
  opts: { signal?: AbortSignal; base?: string; limit?: number } = {},
): Promise<ProfileCommunity[]> {
  const limit = opts.limit ?? 100;
  const url = `${catalystBase(opts.base)}${PATH}?onlyMemberOf=true&limit=${limit}`;
  const res = await signedFetch(identity, url, {
    method: "GET",
    signal: opts.signal,
    headers: { accept: "application/json" },
  });
  if (!res.ok) {
    throw new CatalystError(
      `Catalyst returned ${res.status} ${res.statusText}`,
      url,
      res.status,
    );
  }

  let raw: unknown;
  try {
    raw = await res.json();
  } catch (err) {
    throw new CatalystError(
      `Catalyst returned invalid JSON: ${(err as Error)?.message ?? "parse error"}`,
      url,
      res.status,
    );
  }

  const data = unwrapData<{ results?: unknown[] }>(raw);
  const results = Array.isArray(data?.results) ? data.results : [];

  const out: ProfileCommunity[] = [];
  for (const node of results) {
    const parsed = CommunitySchema.safeParse(node);
    if (!parsed.success) continue;
    const c = parsed.data;
    if (!displayableName(c.name)) continue;
    out.push({
      id: c.id,
      name: c.name,
      membersCount: c.membersCount,
      role: mapRole(c.role),
      thumb: mapThumb(c.thumbnailUrl),
    });
  }
  return out;
}
