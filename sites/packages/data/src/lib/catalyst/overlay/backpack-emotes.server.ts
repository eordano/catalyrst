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

  let ownedUrns: string[] = [];
  let error = false;
  try {
    ownedUrns = await fetchOwnedEmotes(addr, opts);
  } catch {
    error = true;
  }

  let profileEmotes: ProfileEmote[] = [];
  try {
    profileEmotes = await fetchProfileEmotes(addr, opts);
  } catch {
    profileEmotes = [];
  }

  const defUrns = [...new Set([...ownedUrns, ...profileEmotes.map((e) => e.urn)])];
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
