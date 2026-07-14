import { z } from "zod";

import { getJSON, postJSON } from "../client";
import type { GetOptions } from "../client";
import type { AuthIdentity } from "../../auth/types";
import { shortAddress as shortAddressCore } from "../format/address";
import { warnInvalid } from "../warn";

export const BANS_LIMIT = 25;

const ADDR_RE = /^0x[a-fA-F0-9]{40}$/;

export function isAddress(value: string | null | undefined): boolean {
  return typeof value === "string" && ADDR_RE.test(value.trim());
}

export function normalizeAddress(value: string): string {
  return value.trim().toLowerCase();
}

export function shortAddress(value: string): string {
  return shortAddressCore(value.trim());
}

const nullableStr = z.string().nullish().transform((v) => v ?? null);

/**
 * `base_position` and `user_count` are required. Both are on every row of
 * `src/fixtures/operator-scene-bans.json`, which is what
 * `scene-bans.server.ts#loadOperatorPlaces` parses. The old default of "0,0"
 * for a missing position named a real parcel, and 0 for a missing `user_count`
 * claimed an empty scene; a row without them is dropped instead.
 */
export const PlaceRefSchema = z.object({
  id: z.string(),
  title: nullableStr,
  base_position: z.string(),
  parcels: z.number().nullish().transform((v) => v ?? null),
  contact_name: nullableStr,
  image: nullableStr,
  user_count: z.number(),
});

export type PlaceRef = z.infer<typeof PlaceRefSchema>;

export const BanRowSchema = z.object({
  bannedAddress: z.string(),
  name: nullableStr,
});

export type BanRow = z.infer<typeof BanRowSchema>;

/**
 * Required across the board: `catalyrst-comms/src/handlers/scene_bans.rs:105-114`
 * writes all five keys on every response. `results: []` was the dangerous one --
 * a ban list that failed to arrive rendered as a scene with nobody banned.
 */
export const BansEnvelopeSchema = z.object({
  results: z.array(BanRowSchema),
  total: z.number(),
  page: z.number(),
  pages: z.number(),
  limit: z.number(),
});

export type BansPage = z.infer<typeof BansEnvelopeSchema>;

/** Null when the response could not be read -- never an empty ban list. */
export async function loadSceneBans(
  placeId: string,
  opts: GetOptions & { limit?: number; offset?: number } = {},
): Promise<BansPage | null> {
  const { limit = BANS_LIMIT, offset = 0, ...rest } = opts;
  const env = await getJSON<unknown>("/scene-bans", {
    ...rest,
    query: { place_id: placeId, limit, offset, ...(rest.query ?? {}) },
  });
  const parsed = BansEnvelopeSchema.safeParse(env);
  if (!parsed.success) {
    warnInvalid("scene bans envelope", parsed.error.issues);
    return null;
  }
  return parsed.data;
}

export type SceneBanAction = "ban" | "unban";

const SCENE_BAN_METADATA = { signer: "decentraland-kernel-scene" } as const;

export async function commitSceneBan(args: {
  identity: AuthIdentity;
  placeId: string;
  action: SceneBanAction;
  address: string;
  base?: string;
  signal?: AbortSignal;
}): Promise<{ action: SceneBanAction; address: string }> {
  const { identity, placeId, action, address, base, signal } = args;
  const banned_address = normalizeAddress(address);
  if (action === "ban") {
    await postJSON<void>(
      "/scene-bans",
      { place_id: placeId, banned_address },
      { identity, method: "POST", base, metadata: SCENE_BAN_METADATA, signal },
    );
  } else {
    await postJSON<void>(
      "/scene-bans",
      undefined,
      {
        identity,
        method: "DELETE",
        base,
        query: { place_id: placeId, banned_address },
        metadata: SCENE_BAN_METADATA,
        signal,
      },
    );
  }
  return { action, address: banned_address };
}
