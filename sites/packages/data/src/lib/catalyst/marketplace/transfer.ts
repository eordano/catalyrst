import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import type { ProfileWearable as RsProfileWearable } from "@ui/generated/catalyst/market/ProfileWearable";
import { requireAssetsEnvelope, thumbnailFromUrn } from "./account";
import { shortAddress } from "../format/address";
import { warnInvalid } from "../warn";

const nullableStr = z.string().nullish().transform((v) => v ?? null);

export const OwnedElementSchema = z.object({
  id: z.string(),
  urn: nullableStr,
  tokenId: nullableStr,
  category: nullableStr,
  name: nullableStr,
  rarity: nullableStr,
  network: nullableStr,
  contractAddress: z.string().nullish().transform((v) => v ?? null),
  owner: nullableStr,
  image: nullableStr,
  price: nullableStr,
  status: z.string().nullish(),
  unlockAt: z.number().nullish(),
  lease: z.unknown().optional(),
  usageGrant: z.unknown().optional(),
});
export type OwnedElement = z.infer<typeof OwnedElementSchema>;

export function parseOwnedElement(raw: unknown): OwnedElement | null {
  const r = OwnedElementSchema.safeParse(raw);
  if (r.success) return r.data;
  warnInvalid("OwnedElement", r.error.issues);
  return null;
}

export function parseOwnedElements(raw: unknown[]): OwnedElement[] {
  const out: OwnedElement[] = [];
  for (const row of raw ?? []) {
    const el = parseOwnedElement(row);
    if (el) out.push(el);
  }
  return out;
}

export async function fetchOwnedAssets(
  address: string,
  params: { first?: number } = {},
  opts: GetOptions = {},
): Promise<OwnedElement[]> {
  const path = `/market/v1/users/${encodeURIComponent(address)}/wearables`;
  const page = requireAssetsEnvelope(
    await getJSON<unknown>(path, { ...opts, query: { first: params.first ?? 24 } }),
    path,
  );
  return parseOwnedElements(page.elements);
}

export function parseAssetId(
  id: string,
): { contractAddress: string; tokenId: string } | null {
  const dash = id.lastIndexOf("-");
  if (dash <= 0 || dash === id.length - 1) return null;
  return { contractAddress: id.slice(0, dash), tokenId: id.slice(dash + 1) };
}

const KNOWN_CATEGORIES = new Set(["wearable", "emote", "ens", "parcel", "estate"]);
export function coarseCategory(category: string | null | undefined): string {
  if (category && KNOWN_CATEGORIES.has(category)) return category;
  return "wearable";
}

const RARITIES = new Set([
  "common",
  "uncommon",
  "rare",
  "epic",
  "legendary",
  "mythic",
  "unique",
  "exotic",
]);
function safeRarity(r: string | null | undefined): string {
  return r && RARITIES.has(r) ? r : "common";
}

export function toCardNetwork(
  network: string | null | undefined,
): "ethereum" | "polygon" {
  return network === "ETHEREUM" ? "ethereum" : "polygon";
}

export function resolveNetwork(el: {
  network?: string | null;
  urn?: string | null;
}): "ethereum" | "polygon" {
  if (el.network === "ETHEREUM") return "ethereum";
  if (el.network === "MATIC") return "polygon";
  const urn = el.urn ?? "";
  if (/:ethereum:/i.test(urn)) return "ethereum";
  if (/:matic:/i.test(urn)) return "polygon";
  return "polygon";
}

export type TransferAsset = {
  id: string;
  contractAddress: string;
  tokenId: string;
  name: string;
  category: string;
  rarity: string;
  network: "ethereum" | "polygon";
  image: string | null;
};

export function toTransferAsset(el: OwnedElement): TransferAsset {
  const parsed = parseAssetId(el.id);
  return {
    id: el.id,
    contractAddress: el.contractAddress ?? parsed?.contractAddress ?? "",
    tokenId: el.tokenId ?? parsed?.tokenId ?? "",
    name: el.name ?? "Untitled",
    category: coarseCategory(el.category),
    rarity: safeRarity(el.rarity),
    network: resolveNetwork(el),
    image: el.image ?? thumbnailFromUrn(el.urn) ?? null,
  };
}

export function looksLikeAddress(v: string): boolean {
  return /^0x[a-fA-F0-9]{40}$/.test(v.trim());
}

export function shortenHex(v: string): string {
  return v ? shortAddress(v) : v;
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
export type _DriftOwnedElement = Assert<
  AssignableTo<RsProfileWearable, z.input<typeof OwnedElementSchema>>
>;
