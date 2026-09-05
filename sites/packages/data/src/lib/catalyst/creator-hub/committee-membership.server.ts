import { z } from "zod";

import { getJSON } from "../client";

/**
 * `committee` is a required `Vec<CommitteeMemberOut>` on catalyrst-builder's
 * `CurationCollectionsOut`. Defaulting it to `[]` turned any body that was not
 * that payload into "the committee has no members", and every
 * `isCommitteeMember` answer derived from it into a silent `false` -- a denial
 * nobody checked. A rejection now reaches the `catch` below, which returns
 * false only after saying the read failed.
 */
const MembersSchema = z.object({
  committee: z.array(z.object({ address: z.string() })),
});

const TTL_MS = 5 * 60_000;

let cache: { at: number; members: Set<string> } | null = null;
let refreshing: Promise<void> | null = null;

async function fetchMembers(signal?: AbortSignal): Promise<Set<string>> {
  const token =
    typeof process !== "undefined"
      ? process.env?.CATALYRST_BUILDER_ADMIN_TOKEN
      : undefined;
  const raw = await getJSON<{ data?: unknown }>("/v1/collections/curation", {
    signal,
    headers: token ? { authorization: `Bearer ${token}` } : undefined,
  });
  const body = (raw as { data?: unknown })?.data ?? raw;
  const parsed = MembersSchema.parse(body);
  return new Set(parsed.committee.map((m) => m.address.toLowerCase()));
}

export async function isCommitteeMember(
  address: string,
  signal?: AbortSignal,
): Promise<boolean> {
  const wallet = address.trim().toLowerCase();
  if (!wallet) return false;
  const now = Date.now();
  if (!cache) {
    try {
      cache = { at: now, members: await fetchMembers(signal) };
    } catch {
      return false;
    }
  } else if (now - cache.at > TTL_MS && !refreshing) {
    refreshing = fetchMembers()
      .then((members) => {
        cache = { at: Date.now(), members };
      })
      // The refresh keeps the stale membership on failure and leaves `cache.at`
      // untouched, so the next call retries instead of waiting out the TTL.
      .catch(() => undefined)
      .finally(() => {
        refreshing = null;
      });
  }
  return cache.members.has(wallet);
}
