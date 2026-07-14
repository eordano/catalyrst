import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { ETH_ADDRESS_RE } from "../format/address";

export function normalizeAddress(addr: string | null | undefined): string {
  return (addr ?? "").trim().toLowerCase();
}

export function isEthAddress(addr: string): boolean {
  return ETH_ADDRESS_RE.test(addr.trim());
}

export const SLOT_ORDER = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0] as const;
export type SlotNumber = (typeof SLOT_ORDER)[number];

export function isSlotNumber(n: number): n is SlotNumber {
  return Number.isInteger(n) && n >= 0 && n <= 9;
}

export const BASE_EMOTES_PREFIX = "urn:decentraland:off-chain:base-emotes";

export function itemUrn(urn: string): string {
  const parts = urn.split(":");
  const i = parts.indexOf("collections-v2");
  if (i !== -1 && parts.length > i + 3) {
    return parts.slice(0, i + 3).join(":");
  }
  return urn;
}

export function slugToName(urn: string): string {
  const last = urn.split(":").pop() ?? "";
  if (!last || /^\d+$/.test(last)) return "";
  return last
    .replace(/[-_]+/g, " ")
    .replace(/\b\w/g, (m) => m.toUpperCase())
    .trim();
}

export const EMOTE_CATEGORIES = [
  "dance",
  "stunt",
  "greetings",
  "fun",
  "poses",
  "reactions",
  "horror",
  "miscellaneous",
] as const;

/**
 * `category` and `loop` are required on every `emoteDataADR74` in
 * `@dcl/schemas`, so a definition that carries neither is not an emote
 * definition and is dropped by `projectRawEmote` instead of filed under
 * "miscellaneous" as a one-shot. `rarity` is a StandardProps field -- base
 * emotes are prohibited from declaring one -- so its absence is a fact and
 * stays null rather than becoming the literal tier "base".
 */
export const EmoteSchema = z
  .object({
    urn: z.string().min(1),
    name: z.string(),
    description: z.string().nullish().transform((v) => v ?? null),
    thumbnail: z.string().nullish().transform((v) => v ?? null),
    rarity: z.string().nullish().transform((v) => v ?? null),
    category: z.string(),
    loop: z.boolean(),
  })
  .transform((e) => ({
    ...e,
    category: (EMOTE_CATEGORIES as readonly string[]).includes(e.category)
      ? e.category
      : "miscellaneous",
  }));
export type Emote = z.infer<typeof EmoteSchema>;

/** Built by `buildLoadout`, which always supplies a name -- the emote's own or
 *  one read off the URN. Nothing on the wire reaches this schema. */
export const SlotBindingSchema = z.object({
  slot: z.number().int().min(0).max(9),
  urn: z.string().min(1),
  name: z.string(),
});
export type SlotBinding = z.infer<typeof SlotBindingSchema>;

/** See `OwnedElementSchema` in backpack.ts: an unread quantity stays null. */
export const OwnedEmoteElementSchema = z
  .object({
    urn: z.string(),
    amount: z.number().nullish().transform((v) => v ?? null),
  })
  .passthrough();

export function projectRawEmote(raw: unknown): Emote | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const data = (o.emoteDataADR74 ?? {}) as Record<string, unknown>;
  const urn = (o.urn ?? o.id) as unknown;
  const named = typeof o.name === "string" && o.name.trim() !== "";
  const candidate = {
    urn,
    name: named ? o.name : typeof urn === "string" ? slugToName(urn) || urn : o.name,
    description: o.description,
    thumbnail: o.thumbnail,
    rarity: o.rarity,
    category: data.category,
    loop: data.loop,
  };
  const r = EmoteSchema.safeParse(candidate);
  return r.success ? r.data : null;
}

export function parseCatalog(raw: unknown): Emote[] {
  if (!Array.isArray(raw)) return [];
  const out: Emote[] = [];
  for (const item of raw) {
    const e = projectRawEmote(item);
    if (e) out.push(e);
  }
  return out;
}

export function parseOwnedUrns(raw: unknown): string[] {
  if (!Array.isArray(raw)) return [];
  const out: string[] = [];
  for (const item of raw) {
    const r = OwnedEmoteElementSchema.safeParse(item);
    if (r.success) out.push(r.data.urn);
  }
  return out;
}

export async function fetchOwnedEmotes(
  address: string,
  opts: GetOptions = {},
): Promise<string[]> {
  const raw = await getJSON<unknown>(
    `/lambdas/collections/emotes-by-owner/${encodeURIComponent(
      normalizeAddress(address),
    )}`,
    opts,
  );
  return parseOwnedUrns(raw);
}

function extractEmoteDefs(raw: unknown): unknown[] {
  if (Array.isArray(raw)) return raw;
  const d = (raw ?? {}) as { emotes?: unknown[]; data?: unknown[] };
  return d.emotes ?? d.data ?? [];
}

export async function fetchEmoteDefs(
  urns: string[],
  opts: GetOptions = {},
): Promise<Emote[]> {
  const onChain = [...new Set(urns.map(itemUrn))].filter(
    (u) => u && !u.startsWith(BASE_EMOTES_PREFIX),
  );
  if (onChain.length === 0) return [];
  const out: Emote[] = [];
  for (let i = 0; i < onChain.length; i += 40) {
    const chunk = onChain.slice(i, i + 40);
    const qs = chunk.map((u) => `emoteId=${encodeURIComponent(u)}`).join("&");
    try {
      const raw = await getJSON<unknown>(`/lambdas/collections/emotes?${qs}`, opts);
      out.push(...parseCatalog(extractEmoteDefs(raw)));
    } catch {
    }
  }
  return out;
}

const ProfileEmoteSchema = z.object({
  slot: z.number().int().min(0).max(9),
  urn: z.string().min(1),
});
export type ProfileEmote = z.infer<typeof ProfileEmoteSchema>;

export async function fetchProfileEmotes(
  address: string,
  opts: GetOptions = {},
): Promise<ProfileEmote[]> {
  const prof = await getJSON<{
    avatars?: { avatar?: { emotes?: unknown } }[];
  }>(`/lambdas/profiles/${encodeURIComponent(normalizeAddress(address))}`, opts);
  const raw = prof.avatars?.[0]?.avatar?.emotes;
  if (!Array.isArray(raw)) return [];
  const out: ProfileEmote[] = [];
  for (const item of raw) {
    const r = ProfileEmoteSchema.safeParse(item);
    if (r.success) out.push(r.data);
  }
  return out;
}

export type BackpackEmotesData = {
  address: string;
  catalog: Emote[];
  loadout: SlotBinding[];
  slotOrder: number[];
  liveEmpty: boolean;
  source: "live" | "empty" | "error";
  error: boolean;
};

export function sortLoadout(loadout: SlotBinding[]): SlotBinding[] {
  const rank = (slot: number) => (slot === 0 ? 10 : slot);
  return [...loadout].sort((a, b) => rank(a.slot) - rank(b.slot));
}

export function buildLoadout(
  profileEmotes: ProfileEmote[],
  catalog: Emote[],
): SlotBinding[] {
  const byItem = new Map<string, Emote>();
  for (const e of catalog) byItem.set(itemUrn(e.urn), e);
  const out: SlotBinding[] = [];
  for (const { slot, urn } of profileEmotes) {
    if (!isSlotNumber(slot)) continue;
    const def = byItem.get(itemUrn(urn));
    out.push({ slot, urn, name: def?.name || slugToName(urn) });
  }
  return sortLoadout(out);
}

export function rarityLabel(rarity: string): string {
  return rarity.charAt(0).toUpperCase() + rarity.slice(1);
}
