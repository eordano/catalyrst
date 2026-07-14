export type BodyId = "A" | "B";

export type Color3 = { r: number; g: number; b: number };

export type AvatarBase = {
  bodyShapeUrn: string;
  name: string;
  skinColor: Color3;
  hairColor: Color3;
  eyesColor: Color3;
};

const ADJECTIVES = [
  "Cosmic", "Neon", "Astral", "Lunar", "Solar", "Mystic", "Vivid", "Swift",
  "Bold", "Wild", "Prism", "Echo", "Drift", "Flux", "Nova", "Zen",
  "Rune", "Glow", "Vapor", "Hyper",
];
const NAMES = [
  "Morat", "Genesis", "Atlas", "Vega", "Orion", "Pixel", "Wren", "Koda",
  "Sora", "Nyx", "Juno", "Remy", "Indie", "Sage", "Onyx", "Ezra",
  "Lux", "Kai", "Mira", "Bo",
];

function pick<T>(arr: readonly T[]): T {
  const value = arr[Math.floor(Math.random() * arr.length)];
  if (value === undefined) throw new Error("pick(): empty array");
  return value;
}

export function randomName(): string {
  return (pick(ADJECTIVES) + pick(NAMES)).slice(0, 15);
}

export const BODY_SHAPE_URNS: Record<BodyId, string> = {
  A: "urn:decentraland:off-chain:base-avatars:BaseMale",
  B: "urn:decentraland:off-chain:base-avatars:BaseFemale",
};

export const DEFAULT_WEARABLES: Record<BodyId, string[]> = {
  A: [
    "urn:decentraland:off-chain:base-avatars:eyes_00",
    "urn:decentraland:off-chain:base-avatars:eyebrows_00",
    "urn:decentraland:off-chain:base-avatars:mouth_00",
    "urn:decentraland:off-chain:base-avatars:standard_hair",
    "urn:decentraland:off-chain:base-avatars:simple_blue_tshirt",
    "urn:decentraland:off-chain:base-avatars:distressed_black_Jeans",
    "urn:decentraland:off-chain:base-avatars:citycomfortableshoes",
  ],
  B: [
    "urn:decentraland:off-chain:base-avatars:f_eyes_01",
    "urn:decentraland:off-chain:base-avatars:f_eyebrows_00",
    "urn:decentraland:off-chain:base-avatars:f_mouth_00",
    "urn:decentraland:off-chain:base-avatars:standard_hair",
    "urn:decentraland:off-chain:base-avatars:f_sweater",
    "urn:decentraland:off-chain:base-avatars:f_jeans",
    "urn:decentraland:off-chain:base-avatars:bun_shoes",
  ],
};

const SKIN: Color3[] = [
  { r: 0.95, g: 0.83, b: 0.73 }, { r: 0.86, g: 0.68, b: 0.55 },
  { r: 0.7, g: 0.5, b: 0.38 }, { r: 0.5, g: 0.34, b: 0.25 },
  { r: 0.33, g: 0.22, b: 0.16 },
];
const HAIR: Color3[] = [
  { r: 0.1, g: 0.08, b: 0.07 }, { r: 0.35, g: 0.2, b: 0.1 },
  { r: 0.6, g: 0.42, b: 0.2 }, { r: 0.85, g: 0.72, b: 0.4 },
  { r: 0.7, g: 0.1, b: 0.15 }, { r: 0.3, g: 0.3, b: 0.34 },
];
const EYES: Color3[] = [
  { r: 0.25, g: 0.16, b: 0.1 }, { r: 0.4, g: 0.55, b: 0.7 },
  { r: 0.25, g: 0.5, b: 0.3 }, { r: 0.45, g: 0.4, b: 0.36 },
];

export const AVATAR_SKIN_COLORS: readonly Color3[] = SKIN;
export const AVATAR_HAIR_COLORS: readonly Color3[] = HAIR;
export const AVATAR_EYE_COLORS: readonly Color3[] = EYES;

export function randomAvatarBase(name?: string, body?: string): AvatarBase {
  return {
    bodyShapeUrn: BODY_SHAPE_URNS[body as BodyId] || BODY_SHAPE_URNS.A,
    name: name || randomName(),
    skinColor: pick(SKIN),
    hairColor: pick(HAIR),
    eyesColor: pick(EYES),
  };
}
