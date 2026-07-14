import { siteUrl } from "../../data/site";

export type Wearable = {
  urn: string;
  name?: string | null;
  category?: string | null;
  thumbnail?: string | null;
  rarity?: string | null;
  description?: string | null;
  creator?: string | null;
  loop?: boolean | null;
};

export type Equipped = {
  wearables?: string[] | null;
  bodyShape?: string | null;
  name?: string | null;
  skinColor?: string | null;
  hairColor?: string | null;
  eyeColor?: string | null;
  emotes?: string[] | null;
};

export type Base = {
  bodyShape: string;
  name: string;
  skinColor: string;
  hairColor: string;
  eyeColor: string;
};

export type Outfit = {
  slot: number;
  bodyShape?: string;
  wearables?: string[];
  skinColor?: string;
  hairColor?: string;
  eyeColor?: string;
  emotes?: string[];
};

export type LoadoutEntry = { slot: number; urn: string; name?: string | null };

export type ColorCat = "skin" | "hair" | "eyes";

export type SlotDef = { id: string; label: string };

export const SLOTS: SlotDef[] = [
  { id: "body_shape", label: "Body" },
  { id: "hair", label: "Hair" },
  { id: "eyebrows", label: "Eyebrows" },
  { id: "eyes", label: "Eyes" },
  { id: "mouth", label: "Mouth" },
  { id: "facial_hair", label: "Facial Hair" },
  { id: "upper_body", label: "Upper Body" },
  { id: "hands_wear", label: "Handwear" },
  { id: "lower_body", label: "Lower Body" },
  { id: "feet", label: "Feet" },
  { id: "hat", label: "Hat" },
  { id: "eyewear", label: "Eyewear" },
  { id: "earring", label: "Earring" },
  { id: "mask", label: "Mask" },
  { id: "tiara", label: "Tiara" },
  { id: "helmet", label: "Helmet" },
  { id: "top_head", label: "Top Head" },
  { id: "skin", label: "Skin" },
];

export const ICON_ID: Record<string, string> = {
  body_shape: "body",
  facial_hair: "facial",
  upper_body: "upper",
  hands_wear: "hands",
  lower_body: "lower",
  top_head: "hat",
  skin: "body",
};

export function iconFor(id: string): string {
  return ICON_ID[id] ?? id;
}

// Categories that are core avatar attributes rather than catalog-driven -- always shown
// in the rail regardless of what's currently in the catalog.
export const ALWAYS_VISIBLE_CATS = new Set<string>(["body_shape", "skin", "hair", "eyes"]);

export function rarityLabel(r?: string | null): string {
  return r ? r.charAt(0).toUpperCase() + r.slice(1) : "";
}

export function prettyCat(c?: string | null): string {
  return c ? c.replace(/_/g, " ").replace(/\b\w/g, (m) => m.toUpperCase()) : "";
}

function shortenAddr(a?: string | null): string {
  return a && a.length > 12 ? `${a.slice(0, 6)}\u{2026}${a.slice(-4)}` : a || "";
}

export function creatorInfo(w: Wearable): { label: string; url: string } | null {
  if (w?.rarity === "base") return null;
  const p = String(w?.urn || "").split(":");
  if (p.length >= 6 && /^collections-v[12]$/.test(p[3] ?? "")) {
    const url =
      p[3] === "collections-v2"
        ? siteUrl(`/marketplace/${p[4]}-${p[5]}`)
        : siteUrl("/shop");
    return {
      label: w.creator ? shortenAddr(w.creator) : "View on Marketplace",
      url,
    };
  }
  return null;
}

export function openExternal(url: string) {
  if (typeof window !== "undefined")
    window.open(url, "_blank", "noopener,noreferrer");
}
