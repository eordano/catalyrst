import type { Orientation } from "../layout/orientation";

export type MobileOrientation = Orientation;

export type MobileSheetSnap = "peek" | "half" | "full";

export type MobileGlyph =
  | "jump"
  | "hand"
  | "spark"
  | "emote"
  | "chat"
  | "camera";

export type MobileAction = {
  id: string;
  label: string;
  glyph?: MobileGlyph;
  iconUrl?: string;
  hidden?: boolean;
};

export type MobileTab = {
  id: string;
  label: string;
  glyph: MobileTabGlyph;
  badge?: boolean;
  count?: number;
  to?: string;
};

export type MobileTabGlyph =
  | "places"
  | "events"
  | "map"
  | "backpack"
  | "settings"
  | "profile";
