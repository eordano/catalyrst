import {
  buildLoadout,
  fetchEmoteDefs,
  fetchOwnedEmotes,
  fetchProfileEmotes,
  itemUrn,
  normalizeAddress,
  SLOT_ORDER,
  type BackpackEmotesData,
  type ProfileEmote,
} from "./backpack-emotes";
import type { GetOptions } from "../client";

const SLOT_ORDER_LIST: number[] = [...SLOT_ORDER];

function emptyData(
  address: string,
  source: "empty" | "error",
): BackpackEmotesData {
  return {
    address,
    catalog: [],
    loadout: [],
    slotOrder: SLOT_ORDER_LIST,
    liveEmpty: true,
    source,
    error: source === "error",
  };
}

export async function loadBackpackEmotes(
  address: string | null | undefined,
  opts: GetOptions = {},
): Promise<BackpackEmotesData> {
  const addr = normalizeAddress(address);
  if (!addr) return emptyData("", "empty");

  const [ownedRes, profileRes] = await Promise.allSettled([
    fetchOwnedEmotes(addr, opts),
    fetchProfileEmotes(addr, opts),
  ]);
  const ownedUrns: string[] =
    ownedRes.status === "fulfilled" ? ownedRes.value : [];
  const error = ownedRes.status === "rejected";
  const profileEmotes: ProfileEmote[] =
    profileRes.status === "fulfilled" ? profileRes.value : [];

  const defUrns = [
    ...new Set([...ownedUrns, ...profileEmotes.map((e) => e.urn)]),
  ];
  const defs = await fetchEmoteDefs(defUrns, opts);

  const ownedSet = new Set(ownedUrns.map(itemUrn));
  const catalog = defs.filter((e) => ownedSet.has(itemUrn(e.urn)));
  const loadout = buildLoadout(profileEmotes, defs);

  return {
    address: addr,
    catalog,
    loadout,
    slotOrder: SLOT_ORDER_LIST,
    liveEmpty: catalog.length === 0,
    source: error ? "error" : "live",
    error,
  };
}
