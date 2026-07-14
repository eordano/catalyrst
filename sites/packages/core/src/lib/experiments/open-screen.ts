
export const OPEN_SCREEN_EXPERIMENT_KEY = "client_open_screen";

export const OPEN_SCREEN_STORY_DIR = "client/open-screen";

export const OPEN_SCREEN_ARMS = ["base", "genesis", "three-cards"] as const;

export type OpenScreenArm = (typeof OPEN_SCREEN_ARMS)[number];

export type OpenScreenConfig = {
  story: string;
  experimentKey: string;
  variant: string;
  arm: OpenScreenArm;
};

export function activeOpenScreenExperiment(
  raw: string | undefined | null,
): typeof OPEN_SCREEN_STORY_DIR | null {
  const v = raw?.trim();
  if (!v) return null;
  return v === "open-screen" || v === OPEN_SCREEN_STORY_DIR || v === OPEN_SCREEN_EXPERIMENT_KEY
    ? OPEN_SCREEN_STORY_DIR
    : null;
}

export function openScreenFromFlags(flags: Record<string, unknown>): OpenScreenArm | null {
  const arm = flags["openScreen"];
  if (typeof arm !== "string") return null;
  return (OPEN_SCREEN_ARMS as readonly string[]).includes(arm)
    ? (arm as OpenScreenArm)
    : null;
}

export { armOverride } from "./create-entry";

export const OPEN_SCREEN_TARGETS = {
  explore: "/places",
  avatar: "/bevy-overlay/backpack-equip?from=open-screen",
} as const;

export function placeJumpPath(placeId: string): string {
  return `/places/${encodeURIComponent(placeId)}?from=open-screen`;
}
