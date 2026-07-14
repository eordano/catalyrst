import type {
  CompositeComponentBlock,
  SceneComposite,
} from "../catalyst/creator-hub/scene-composite";
import { serializeSceneComposite } from "../catalyst/creator-hub/scene-composite";

export const BUILDER_ITEMS_PREFIX = "/builder-items/";
export const TEMPLATE_ASSET_DIR = "assets/imported/template-assets";

const TRANSFORM = "core::Transform";
const NAME = "core-schema::Name";
const GLTF = "core::GltfContainer";
const MESH_RENDERER = "core::MeshRenderer";
const MATERIAL = "core::Material";
const TEXT_SHAPE = "core::TextShape";

type Vec3 = { x: number; y: number; z: number };
type Color4 = { r: number; g: number; b: number; a: number };

type EntitySpec = {
  id: number;
  name: string;
  parent?: number;
  position: [number, number, number];
  rotY?: number;
  scale?: [number, number, number];
  glb?: string;
  mesh?: "box" | "plane";
  color?: Color4 & { emissive?: boolean };
  text?: { value: string; fontSize?: number; color?: Color4 };
};

function vec3(v: [number, number, number]): Vec3 {
  return { x: v[0], y: v[1], z: v[2] };
}

function quatY(deg: number): { x: number; y: number; z: number; w: number } {
  const half = (deg * Math.PI) / 360;
  return { x: 0, y: Math.sin(half), z: 0, w: Math.cos(half) };
}

function buildComposite(specs: EntitySpec[]): SceneComposite {
  const blocks = new Map<string, CompositeComponentBlock>();
  const put = (component: string, id: number, value: unknown) => {
    let block = blocks.get(component);
    if (!block) {
      block = { name: component, data: {} };
      blocks.set(component, block);
    }
    block.data[String(id)] = { json: value };
  };

  for (const s of specs) {
    put(TRANSFORM, s.id, {
      position: vec3(s.position),
      rotation: quatY(s.rotY ?? 0),
      scale: vec3(s.scale ?? [1, 1, 1]),
      parent: s.parent ?? 0,
    });
    put(NAME, s.id, { value: s.name });
    if (s.glb) {
      put(GLTF, s.id, { src: `${TEMPLATE_ASSET_DIR}/${s.glb}.glb` });
    }
    if (s.mesh) {
      put(MESH_RENDERER, s.id, {
        mesh:
          s.mesh === "box"
            ? { $case: "box", box: { uvs: [] } }
            : { $case: "plane", plane: { uvs: [] } },
      });
      if (s.color) {
        const { r, g, b, a, emissive } = s.color;
        put(MATERIAL, s.id, {
          material: {
            $case: "pbr",
            pbr: {
              albedoColor: { r, g, b, a },
              metallic: 0,
              roughness: 0.6,
              ...(emissive
                ? { emissiveColor: { r, g, b }, emissiveIntensity: 2 }
                : {}),
            },
          },
        });
      }
    }
    if (s.text) {
      put(TEXT_SHAPE, s.id, {
        text: s.text.value,
        fontSize: s.text.fontSize ?? 3,
        textColor: s.text.color ?? { r: 1, g: 1, b: 1, a: 1 },
      });
    }
  }

  return { version: 1, components: Array.from(blocks.values()) };
}

const GLB = {
  groundCrackedStones: "QmWM8PmLyebMayuY3YkKLX9mZoAAWb9kk69hueAyTwfkvq",
  groundCracked: "QmcdYn4nztdKrdXivGn8W6qXCL477hzP7CzWkvfxbW2cAT",
  groundStones: "QmWNkKw16ddLB2Kc5JrjJTm7cPTydgTZ7BV52tNTKQwAjC",
  groundGlossyTiles: "QmNXC8u1CdqomJRpUmbbf3RZeVg1msoqjNspxR8WFXnAJd",
  groundSandGold: "QmcBC3imiVdkJt9hiS6wLcqKaioUXCkU5d3JMVXuE7Czmc",
  roadCobbleShort: "QmZB1QiqFdAri14u7enHCgc6umzy54BDYMuv5V8Zx46Koi",
  ironDoor: "QmYtYTFecuismor7P1J1XQJGCkf8MPgTu8A1kKCc4eZeTu",
  ironFence1: "QmePiWso4fZxFLLEr8XFjcbv64oycL8a4TYoE2c6Nc91Mk",
  ironFence2: "QmdDzjWt9EymQucZhvB856zqp2VZrFiW1b1LK1B2BAFeBQ",
  lamp01: "QmZttTWRiRGJx6XGpBJ6EpAoi7RNzQgkLfxbjggSB7yLVs",
  lamp02: "QmNwVpTBJveQG7EmyZ2LyVUgbRCbvi4mLfToTKmfJwjnTn",
  spider01: "Qmdsn5Ay7YboWasEzSQ4gM6REP7mmcEBJjAC5qjSko31vr",
  spider02: "QmW7Nj2CNte1QwrW76DqrqKHfPGsENCu1R9aQgAQdYaatM",
  skeleton: "QmdP56MahpC6N7XAMLL9YXgAwqytq7jA6ekTYvozvWMNd8",
  skullDeco: "QmaBhD27CTATq44aYFJn42P8GVMq8b48T5eCNFgKY5Bgxj",
  wallPlainWhite: "QmVSPBDTuHVjpCsAnnWLetdEvEh7jLd94eDjnHzT6hwZ1T",
  pillarAntique: "QmPby7C8vYR3wVwVHmKdHNriLo2ZeB4YJTRs6YFoJrav8M",
  artworkInfo: "QmS6ZhFSA3PviACxK4hbmcZ6kTvVRjynkRMMn44VcbCw1R",
  consoleTable: "QmUbbPZdYVk1CjtaQdzSiLxb7eCZ7Uez1LkbaCyeNupspP",
  wallDarkGrey: "QmdH5T8VqCqxvVjdcUTV6fzXhvb2mnei83WythvjyKnYxa",
  lever: "QmTFBLZKDLfebCi7UyqQzE2WWA567enFb8HVuyw6BFVXUM",
  chestPlates: "QmfDo8Ddna9p3PDMtaSd8fwZADyhNKEoXAQo2fhkR6jacP",
  chestKey: "QmdUa7A9JT81AEU8FF2zQmbZPYmJuz4wAWySNMppBdzs2e",
  wallClock: "QmYVhU2QRvs9NfEC67rEQ1M4gKdrnGVo8ZfF2DcR1bfYmB",
  bookshelf: "QmVy3RiS4Sj2coFgvyYgimFgyLJPLFmgYzAHAXPo6uS7Es",
  candle02: "QmYH4SboLRWVCnQ9EJEMEZHfUDBERbjRVhj5cWdsLGT4ma",
  candle03: "Qmd1422xaQVfN5Qdo3UZusyPYGDkWV1yR7PZdNxpZaaPc7",
  arcadeBlack: "QmVGASrMSePtdbZCykiYx9xmJ9UM1AaqQa4ZPfN2cGbWYQ",
  arcadeRed: "QmaY9uE5Bhu4EfwDr8nG7bFYrv3Fv2wsK2GocUoaVedFaV",
  arcadeGreen: "QmeyfMr2qCLTqXcDrykTW27LkwXr3WSyj4gpUEdQqGwk9R",
  arcadeBlue: "QmVA4F78aMUFiZBhmADXb6yvo7YoXnX2PiqCX1oJNMR3sW",
  buttonsTable: "QmZBXnGFrbdDzmFdQU8KWJwocsPQo6LSF8ZPa5JrZatEWK",
  chestWestern: "QmbtVnAHdPz2d1VA8rV4NfgpDjZtpGnaUotZAdirsZPKRc",
  barrel2: "QmXFxidJLGywuf4dUBfwPihbGjxpL8Fyj2KxikCbENMi1w",
  cactus: "QmTgk1AHuFvuWGg1ZuXDC64Cc3JCXDqzXkJK9haSbvhJMk",
  solidWater: "QmVME9yNtjo6JQZ79ybc392rKJo7aYzQU1pNDs3onq4yek",
} as const;

const SPAWN: EntitySpec = { id: 512, name: "Spawn Point", position: [2, 0, 2] };

const LAYOUTS: Record<string, EntitySpec[]> = {
  "tower-defense": [
    SPAWN,
    { id: 513, name: "Stone Ground", position: [8, 0, 8], glb: GLB.groundCrackedStones },
    { id: 514, name: "Path Segment A", position: [8, 0.01, 3.5], glb: GLB.roadCobbleShort },
    { id: 515, name: "Path Segment B", position: [8, 0.01, 6.5], glb: GLB.roadCobbleShort },
    { id: 516, name: "Path Segment C", position: [8, 0.01, 9.5], glb: GLB.roadCobbleShort },
    { id: 517, name: "Path Segment D", position: [8, 0.01, 12.5], glb: GLB.roadCobbleShort },
    { id: 518, name: "Spawn Gate", position: [8, 0, 15], rotY: 180, glb: GLB.ironDoor },
    { id: 519, name: "Gate Fence Left", position: [4.5, 0, 15], glb: GLB.ironFence1 },
    { id: 520, name: "Gate Fence Right", position: [11.5, 0, 15], glb: GLB.ironFence2 },
    { id: 521, name: "Watch Post West", position: [5, 0, 7], glb: GLB.lamp01 },
    { id: 522, name: "Watch Post East", position: [11, 0, 7], glb: GLB.lamp02 },
    { id: 523, name: "Creep Spider A", position: [8, 0, 12.5], rotY: 180, glb: GLB.spider01 },
    { id: 524, name: "Creep Spider B", position: [7.2, 0, 9.5], rotY: 200, glb: GLB.spider02 },
    { id: 525, name: "Fallen Defender", position: [9.5, 0, 5], rotY: 70, glb: GLB.skeleton },
    { id: 526, name: "Trap Marker", position: [8, 0, 6], glb: GLB.skullDeco },
  ],

  "nft-art-wall": [
    SPAWN,
    { id: 513, name: "Gallery Floor", position: [8, 0, 8], glb: GLB.groundStones },
    { id: 514, name: "Gallery Wall Left", position: [4, 0, 15], glb: GLB.wallPlainWhite },
    { id: 515, name: "Gallery Wall Center", position: [8, 0, 15], glb: GLB.wallPlainWhite },
    { id: 516, name: "Gallery Wall Right", position: [12, 0, 15], glb: GLB.wallPlainWhite },
    {
      id: 517,
      name: "Canvas 1",
      position: [2, 2, 14.7],
      rotY: 180,
      scale: [2.2, 2.2, 1],
      mesh: "plane",
      color: { r: 0.9, g: 0.35, b: 0.25, a: 1, emissive: true },
    },
    {
      id: 518,
      name: "Canvas 2",
      position: [6, 2, 14.7],
      rotY: 180,
      scale: [2.2, 2.2, 1],
      mesh: "plane",
      color: { r: 0.25, g: 0.65, b: 0.95, a: 1, emissive: true },
    },
    {
      id: 519,
      name: "Canvas 3",
      position: [10, 2, 14.7],
      rotY: 180,
      scale: [2.2, 2.2, 1],
      mesh: "plane",
      color: { r: 0.55, g: 0.9, b: 0.4, a: 1, emissive: true },
    },
    { id: 520, name: "Pillar West", position: [1.5, 0, 13.5], glb: GLB.pillarAntique },
    { id: 521, name: "Pillar East", position: [14.5, 0, 13.5], glb: GLB.pillarAntique },
    { id: 522, name: "Artwork Info", position: [12.5, 0, 13.8], rotY: 180, glb: GLB.artworkInfo },
    { id: 523, name: "Console Table", position: [8, 0, 11.5], glb: GLB.consoleTable },
  ],

  "escape-room": [
    SPAWN,
    { id: 513, name: "Stone Floor", position: [8, 0, 8], glb: GLB.groundCracked },
    { id: 514, name: "Back Wall Left", position: [2, 0, 15.2], glb: GLB.wallDarkGrey },
    { id: 515, name: "Back Wall Right", position: [10, 0, 15.2], glb: GLB.wallDarkGrey },
    { id: 516, name: "Locked Door", position: [8, 0, 15], rotY: 180, glb: GLB.ironDoor },
    { id: 517, name: "Escape Lever", position: [12.5, 0, 12], rotY: 270, glb: GLB.lever },
    { id: 518, name: "Puzzle Chest", position: [3.5, 0, 12], rotY: 135, glb: GLB.chestPlates },
    { id: 519, name: "Brass Key", position: [3.5, 1.1, 12], scale: [1.4, 1.4, 1.4], glb: GLB.chestKey },
    { id: 520, name: "Countdown Clock", position: [11.2, 0, 14.4], rotY: 180, glb: GLB.wallClock },
    { id: 521, name: "Bookshelf", position: [1.5, 0, 14], scale: [2, 2, 2], glb: GLB.bookshelf },
    { id: 522, name: "Candle Left", position: [6.8, 0, 14.4], glb: GLB.candle03 },
    { id: 523, name: "Candle Right", position: [9.2, 0, 14.4], glb: GLB.candle02 },
  ],

  "memory-game": [
    SPAWN,
    { id: 513, name: "Arcade Floor", position: [8, 0, 8], glb: GLB.groundGlossyTiles },
    { id: 514, name: "Arcade \u{2014} Black", position: [3.5, 0, 13.5], rotY: 180, glb: GLB.arcadeBlack },
    { id: 515, name: "Arcade \u{2014} Red", position: [6.5, 0, 13.5], rotY: 180, glb: GLB.arcadeRed },
    { id: 516, name: "Arcade \u{2014} Green", position: [9.5, 0, 13.5], rotY: 180, glb: GLB.arcadeGreen },
    { id: 517, name: "Arcade \u{2014} Blue", position: [12.5, 0, 13.5], rotY: 180, glb: GLB.arcadeBlue },
    { id: 518, name: "Control Table", position: [8, 0, 10.8], rotY: 180, glb: GLB.buttonsTable },
    {
      id: 519,
      name: "Pad Red",
      position: [6.2, 0.15, 6.2],
      scale: [1.6, 0.3, 1.6],
      mesh: "box",
      color: { r: 0.85, g: 0.1, b: 0.1, a: 1, emissive: true },
    },
    {
      id: 520,
      name: "Pad Green",
      position: [9.8, 0.15, 6.2],
      scale: [1.6, 0.3, 1.6],
      mesh: "box",
      color: { r: 0.1, g: 0.8, b: 0.2, a: 1, emissive: true },
    },
    {
      id: 521,
      name: "Pad Blue",
      position: [6.2, 0.15, 3.6],
      scale: [1.6, 0.3, 1.6],
      mesh: "box",
      color: { r: 0.15, g: 0.3, b: 0.9, a: 1, emissive: true },
    },
    {
      id: 522,
      name: "Pad Yellow",
      position: [9.8, 0.15, 3.6],
      scale: [1.6, 0.3, 1.6],
      mesh: "box",
      color: { r: 0.95, g: 0.85, b: 0.1, a: 1, emissive: true },
    },
  ],

  "castaway-2048": [
    SPAWN,
    { id: 513, name: "Beach Sand", position: [8, 0, 8], glb: GLB.groundSandGold },
    {
      id: 514,
      name: "Game Board",
      position: [8, 0.15, 8],
      scale: [6.5, 0.3, 6.5],
      mesh: "box",
      color: { r: 0.35, g: 0.23, b: 0.12, a: 1 },
    },
    {
      id: 515,
      name: "Tile 2",
      position: [6.6, 0.5, 6.6],
      scale: [1.4, 0.4, 1.4],
      mesh: "box",
      color: { r: 0.93, g: 0.89, b: 0.85, a: 1 },
    },
    {
      id: 517,
      name: "Tile 4",
      position: [9.4, 0.5, 6.6],
      scale: [1.4, 0.4, 1.4],
      mesh: "box",
      color: { r: 0.93, g: 0.85, b: 0.65, a: 1 },
    },
    {
      id: 519,
      name: "Tile 8",
      position: [6.6, 0.5, 9.4],
      scale: [1.4, 0.4, 1.4],
      mesh: "box",
      color: { r: 0.95, g: 0.6, b: 0.35, a: 1 },
    },
    {
      id: 521,
      name: "Tile 16",
      position: [9.4, 0.5, 9.4],
      scale: [1.4, 0.4, 1.4],
      mesh: "box",
      color: { r: 0.93, g: 0.45, b: 0.25, a: 1 },
    },
    { id: 523, name: "Treasure Chest", position: [13.5, 0, 13.5], rotY: 225, glb: GLB.chestWestern },
    { id: 524, name: "Washed-up Barrel", position: [2.5, 0, 13], glb: GLB.barrel2 },
    { id: 525, name: "Cactus", position: [14, 0, 3], scale: [1.5, 1.5, 1.5], glb: GLB.cactus },
    { id: 526, name: "Shoreline", position: [8, 0.05, 0.8], scale: [15.5, 0.15, 1.5], glb: GLB.solidWater },
  ],
};

export type TemplateContentMeta = {
  id: string;
  title: string;
  githubLink: string;
  readmeNote: string;
};

const META: Record<string, TemplateContentMeta> = {
  "tower-defense": {
    id: "tower-defense",
    title: "Tower Defense",
    githubLink: "https://github.com/decentraland-scenes/Tower-defense",
    readmeNote:
      "This scaffold contains a curated tower-defense scene layout (cobbled path, " +
      "spawn gate, watch posts and creep spiders placed from the builder catalog) " +
      "plus a small SDK7 starter script (`src/index.ts`) that marches the creeps " +
      "down the path and lets you click them to knock them back. It is NOT a port of " +
      "the original Tower-defense game logic \u{2014} the full original scene (SDK6) lives " +
      "at the link above.",
  },
  "nft-art-wall": {
    id: "nft-art-wall",
    title: "Art Wall",
    githubLink: "https://github.com/decentraland-scenes/nft-wall-example-scene",
    readmeNote:
      "This scaffold contains a curated gallery layout (white gallery walls, pillars " +
      "and three placeholder canvases) plus a small SDK7 starter script " +
      "(`src/index.ts`) that cycles each canvas colour on click and shows how to swap " +
      "a canvas for an `NftShape` with your own token. It is NOT a port of the " +
      "original NFT-wall code \u{2014} the full original scene (SDK6) lives at the link above.",
  },
  "escape-room": {
    id: "escape-room",
    title: "Escape Room",
    githubLink: "https://github.com/decentraland-scenes/Escape-Room",
    readmeNote:
      "This scaffold contains a curated escape-room layout (locked iron door, lever, " +
      "puzzle chest with a key, countdown clock and candle-lit props from the builder " +
      "catalog) plus a small SDK7 starter script (`src/index.ts`): pick up the key, " +
      "pull the lever, the door opens. It is NOT a port of the original 9-room " +
      "Escape-Room game \u{2014} the full original scene (SDK6) lives at the link above.",
  },
  "memory-game": {
    id: "memory-game",
    title: "Memory Game",
    githubLink: "https://github.com/decentraland-scenes/Memory-game",
    readmeNote:
      "This scaffold contains a curated arcade layout (four arcade machines and four " +
      "Simon-style colour pads) plus a small SDK7 starter script (`src/index.ts`) " +
      "that flashes a growing pad sequence and checks your clicks. It is a fresh SDK7 " +
      "starter inspired by \u{2014} not a port of \u{2014} the original Memory game (SDK6), which " +
      "lives at the link above.",
  },
  "castaway-2048": {
    id: "castaway-2048",
    title: "Castaway 2048",
    githubLink: "https://github.com/decentraland-scenes/Castaway-2048",
    readmeNote:
      "This scaffold contains a curated beach layout (sand ground, shoreline, " +
      "treasure chest and a 2048 board with four starting tiles) plus a small SDK7 " +
      "starter script (`src/index.ts`) that labels each tile and doubles its value " +
      "on click, 2048-style. It is NOT a port of the original Castaway 2048 game " +
      "logic \u{2014} the full original scene (SDK6) lives at the link above.",
  },
};

const STARTER_HEADER = (title: string, github: string) =>
  `// ${title} \u{2014} SDK7 starter scaffolded by the Decentraland Creator Hub.\n` +
  `// This is a small self-contained starter that drives the entities placed in\n` +
  `// this template's main.composite (matched by their Name component).\n` +
  `// It is NOT a port of the original scene: the full original ${title}\n` +
  `// (SDK6) lives at ${github}\n`;

const COMMON_IMPORTS =
  `import {\n` +
  `  engine,\n` +
  `  Entity,\n` +
  `  Name,\n` +
  `  Transform,\n` +
  `  Material,\n` +
  `  TextShape,\n` +
  `  MeshCollider,\n` +
  `  pointerEventsSystem,\n` +
  `  InputAction,\n` +
  `} from '@dcl/sdk/ecs'\n` +
  `import { Color3, Color4, Vector3 } from '@dcl/sdk/math'\n` +
  `import { initAssetPacks } from '@dcl/asset-packs/dist/scene-entrypoint'\n\n` +
  `// Runs no-code Actions/Triggers (smart items) authored in the editor.\n` +
  `initAssetPacks(engine)\n\n` +
  `/** Find a template entity by the Name the editor shows in the tree. */\n` +
  `function byName(value: string): Entity | null {\n` +
  `  for (const [entity, name] of engine.getEntitiesWith(Name)) {\n` +
  `    if (name.value === value) return entity\n` +
  `  }\n` +
  `  return null\n` +
  `}\n\n` +
  `/** Floating status text, created at runtime by this script. */\n` +
  `function makeStatusBoard(text: string, position: Vector3): Entity {\n` +
  `  const board = engine.addEntity()\n` +
  `  Transform.create(board, { position })\n` +
  `  TextShape.create(board, { text, fontSize: 3 })\n` +
  `  return board\n` +
  `}\n\n`;

const STARTERS: Record<string, (name: string) => string> = {
  "tower-defense": () =>
    COMMON_IMPORTS +
    `const CREEPS = ['Creep Spider A', 'Creep Spider B']\n` +
    `const GATE_Z = 14 // creeps respawn at the gate\n` +
    `const GOAL_Z = 3 // ...and try to reach this end of the path\n` +
    `const SPEED = 0.8 // metres per second\n\n` +
    `export function main() {\n` +
    `  const board = makeStatusBoard('TOWER DEFENSE\\nWave 1 \u{2014} hold the path!', Vector3.create(8, 2.8, 13.5))\n` +
    `  let breaches = 0\n\n` +
    `  for (const creepName of CREEPS) {\n` +
    `    const creep = byName(creepName)\n` +
    `    if (!creep) continue\n` +
    `    // Click a creep to knock it back to the spawn gate.\n` +
    `    pointerEventsSystem.onPointerDown(\n` +
    `      { entity: creep, opts: { button: InputAction.IA_POINTER, hoverText: 'Repel!' } },\n` +
    `      () => {\n` +
    `        Transform.getMutable(creep).position.z = GATE_Z\n` +
    `      }\n` +
    `    )\n` +
    `  }\n\n` +
    `  // March every creep down the path toward the goal.\n` +
    `  engine.addSystem((dt) => {\n` +
    `    for (const creepName of CREEPS) {\n` +
    `      const creep = byName(creepName)\n` +
    `      if (!creep) continue\n` +
    `      const t = Transform.getMutable(creep)\n` +
    `      t.position.z -= SPEED * dt\n` +
    `      if (t.position.z <= GOAL_Z) {\n` +
    `        t.position.z = GATE_Z // breached! back to the gate\n` +
    `        breaches += 1\n` +
    `        TextShape.getMutable(board).text = 'TOWER DEFENSE\\nBreaches: ' + breaches + ' \u{2014} defend the path!'\n` +
    `      }\n` +
    `    }\n` +
    `  })\n` +
    `}\n`,

  "nft-art-wall": () =>
    COMMON_IMPORTS +
    `const CANVASES = ['Canvas 1', 'Canvas 2', 'Canvas 3']\n` +
    `const PALETTE = [\n` +
    `  Color4.create(0.9, 0.35, 0.25, 1),\n` +
    `  Color4.create(0.25, 0.65, 0.95, 1),\n` +
    `  Color4.create(0.55, 0.9, 0.4, 1),\n` +
    `  Color4.create(0.95, 0.8, 0.2, 1),\n` +
    `  Color4.create(0.75, 0.3, 0.9, 1),\n` +
    `]\n\n` +
    `export function main() {\n` +
    `  CANVASES.forEach((canvasName, i) => {\n` +
    `    const canvas = byName(canvasName)\n` +
    `    if (!canvas) return\n` +
    `    let colorIndex = i\n` +
    `    // A canvas needs a collider to be clickable.\n` +
    `    MeshCollider.setPlane(canvas)\n` +
    `    pointerEventsSystem.onPointerDown(\n` +
    `      { entity: canvas, opts: { button: InputAction.IA_POINTER, hoverText: 'Cycle art' } },\n` +
    `      () => {\n` +
    `        colorIndex = (colorIndex + 1) % PALETTE.length\n` +
    `        const c = PALETTE[colorIndex]\n` +
    `        Material.setPbrMaterial(canvas, {\n` +
    `          albedoColor: c,\n` +
    `          emissiveColor: Color3.create(c.r, c.g, c.b),\n` +
    `          emissiveIntensity: 2,\n` +
    `        })\n` +
    `      }\n` +
    `    )\n` +
    `  })\n\n` +
    `  // To show a real NFT instead of a placeholder canvas, replace the\n` +
    `  // MeshRenderer plane with an NftShape pointing at your token:\n` +
    `  //   NftShape.create(canvas, { urn: 'urn:decentraland:ethereum:erc721:<contract>:<tokenId>' })\n` +
    `}\n`,

  "escape-room": () =>
    COMMON_IMPORTS +
    `export function main() {\n` +
    `  const door = byName('Locked Door')\n` +
    `  const lever = byName('Escape Lever')\n` +
    `  const key = byName('Brass Key')\n` +
    `  const hint = makeStatusBoard('Find the key.\\nPull the lever.\\nBeat the clock.', Vector3.create(8, 2.9, 13.4))\n\n` +
    `  let hasKey = false\n` +
    `  let doorOpen = false\n\n` +
    `  if (key) {\n` +
    `    pointerEventsSystem.onPointerDown(\n` +
    `      { entity: key, opts: { button: InputAction.IA_POINTER, hoverText: 'Take the key' } },\n` +
    `      () => {\n` +
    `        hasKey = true\n` +
    `        Transform.getMutable(key).scale = { x: 0, y: 0, z: 0 } // pocketed\n` +
    `        TextShape.getMutable(hint).text = 'You have the key.\\nNow pull the lever.'\n` +
    `      }\n` +
    `    )\n` +
    `  }\n\n` +
    `  if (lever) {\n` +
    `    pointerEventsSystem.onPointerDown(\n` +
    `      { entity: lever, opts: { button: InputAction.IA_POINTER, hoverText: 'Pull the lever' } },\n` +
    `      () => {\n` +
    `        if (!hasKey) {\n` +
    `          TextShape.getMutable(hint).text = 'The lever is jammed.\\nFind the key first.'\n` +
    `          return\n` +
    `        }\n` +
    `        if (doorOpen || !door) return\n` +
    `        doorOpen = true\n` +
    `        // Slide the door underground to open the way out.\n` +
    `        Transform.getMutable(door).position.y = -3.2\n` +
    `        TextShape.getMutable(hint).text = 'The door grinds open.\\nYou escaped!'\n` +
    `      }\n` +
    `    )\n` +
    `  }\n` +
    `}\n`,

  "memory-game": () =>
    COMMON_IMPORTS +
    `const PADS = ['Pad Red', 'Pad Green', 'Pad Blue', 'Pad Yellow']\n\n` +
    `export function main() {\n` +
    `  const sequence: number[] = []\n` +
    `  let guessIndex = 0\n` +
    `  let showing = false\n\n` +
    `  const board = makeStatusBoard('MEMORY GAME', Vector3.create(8, 3, 12.8))\n` +
    `  const setBoard = (text: string) => {\n` +
    `    TextShape.getMutable(board).text = text\n` +
    `  }\n\n` +
    `  const flash = (pad: Entity, after: () => void) => {\n` +
    `    const t = Transform.getMutable(pad)\n` +
    `    const baseY = t.position.y\n` +
    `    t.position.y = baseY + 0.4\n` +
    `    let elapsed = 0\n` +
    `    const sys = (dt: number) => {\n` +
    `      elapsed += dt\n` +
    `      if (elapsed >= 0.45) {\n` +
    `        Transform.getMutable(pad).position.y = baseY\n` +
    `        engine.removeSystem(sys)\n` +
    `        after()\n` +
    `      }\n` +
    `    }\n` +
    `    engine.addSystem(sys)\n` +
    `  }\n\n` +
    `  const showSequence = (i = 0) => {\n` +
    `    showing = true\n` +
    `    if (i >= sequence.length) {\n` +
    `      showing = false\n` +
    `      guessIndex = 0\n` +
    `      setBoard('MEMORY GAME\\nYour turn \u{2014} round ' + sequence.length)\n` +
    `      return\n` +
    `    }\n` +
    `    const pad = byName(PADS[sequence[i]])\n` +
    `    if (!pad) return\n` +
    `    flash(pad, () => showSequence(i + 1))\n` +
    `  }\n\n` +
    `  const nextRound = () => {\n` +
    `    sequence.push(Math.floor(Math.random() * PADS.length))\n` +
    `    setBoard('MEMORY GAME\\nWatch the pads...')\n` +
    `    showSequence()\n` +
    `  }\n\n` +
    `  PADS.forEach((padName, padIndex) => {\n` +
    `    const pad = byName(padName)\n` +
    `    if (!pad) return\n` +
    `    MeshCollider.setBox(pad)\n` +
    `    pointerEventsSystem.onPointerDown(\n` +
    `      { entity: pad, opts: { button: InputAction.IA_POINTER, hoverText: padName } },\n` +
    `      () => {\n` +
    `        if (showing || sequence.length === 0) return\n` +
    `        if (sequence[guessIndex] === padIndex) {\n` +
    `          guessIndex += 1\n` +
    `          if (guessIndex >= sequence.length) nextRound()\n` +
    `        } else {\n` +
    `          setBoard('MEMORY GAME\\nWrong pad! Score: ' + (sequence.length - 1) + ' \u{2014} starting over')\n` +
    `          sequence.length = 0\n` +
    `          guessIndex = 0\n` +
    `        }\n` +
    `      }\n` +
    `    )\n` +
    `  })\n\n` +
    `  nextRound()\n` +
    `}\n`,

  "castaway-2048": () =>
    COMMON_IMPORTS +
    `const TILES = ['Tile 2', 'Tile 4', 'Tile 8', 'Tile 16']\n` +
    `const WIN = 2048\n\n` +
    `// 2048-style colour ramp (paler \u{2192} deeper as the value doubles)\n` +
    `function tileColor(value: number): Color4 {\n` +
    `  const step = Math.min(Math.log2(value), 11) / 11\n` +
    `  return Color4.create(0.93, 0.89 - 0.5 * step, 0.85 - 0.7 * step, 1)\n` +
    `}\n\n` +
    `export function main() {\n` +
    `  const values = new Map<string, number>()\n` +
    `  const board = makeStatusBoard('CASTAWAY 2048\\nMerge tiles, open the chest', Vector3.create(8, 3.2, 11.8))\n` +
    `  const labels = new Map<string, Entity>()\n\n` +
    `  TILES.forEach((tileName) => {\n` +
    `    const start = parseInt(tileName.split(' ')[1], 10) || 2\n` +
    `    values.set(tileName, start)\n` +
    `    const tile = byName(tileName)\n` +
    `    if (!tile) return\n` +
    `    const pos = Transform.get(tile).position\n` +
    `    labels.set(tileName, makeStatusBoard(String(start), Vector3.create(pos.x, pos.y + 0.9, pos.z)))\n` +
    `    MeshCollider.setBox(tile)\n` +
    `    // Starter mechanic: click a tile to double it, 2048-style. Replace this\n` +
    `    // with real slide-and-merge logic as you build the game out.\n` +
    `    pointerEventsSystem.onPointerDown(\n` +
    `      { entity: tile, opts: { button: InputAction.IA_POINTER, hoverText: 'Merge' } },\n` +
    `      () => {\n` +
    `        const next = (values.get(tileName) ?? 2) * 2\n` +
    `        values.set(tileName, next)\n` +
    `        Material.setPbrMaterial(tile, { albedoColor: tileColor(next) })\n` +
    `        const label = labels.get(tileName)\n` +
    `        if (label) TextShape.getMutable(label).text = String(next)\n` +
    `        if (next >= WIN) {\n` +
    `          TextShape.getMutable(board).text = 'CASTAWAY 2048\\nYou made ' + next + ' \u{2014} open the chest!'\n` +
    `        }\n` +
    `      }\n` +
    `    )\n` +
    `  })\n` +
    `}\n`,
};

export const TEMPLATE_COMPOSITE_IDS = Object.keys(LAYOUTS);

export function hasTemplateComposite(template: string | undefined | null): boolean {
  return !!template && template in LAYOUTS;
}

export function templateContentMeta(template: string): TemplateContentMeta | null {
  return META[template] ?? null;
}

export function buildTemplateComposite(template: string): SceneComposite | null {
  const specs = LAYOUTS[template];
  if (!specs) return null;
  return buildComposite(specs);
}

export function buildTemplateCompositeText(template: string): string | null {
  const composite = buildTemplateComposite(template);
  return composite ? serializeSceneComposite(composite) : null;
}

export function templateAssetContents(template: string): Record<string, string> {
  const specs = LAYOUTS[template];
  const out: Record<string, string> = {};
  if (!specs) return out;
  for (const s of specs) {
    if (s.glb) out[`${TEMPLATE_ASSET_DIR}/${s.glb}.glb`] = s.glb;
  }
  return out;
}

export function templateIndexTs(template: string, _sceneName?: string): string | null {
  const starter = STARTERS[template];
  const meta = META[template];
  if (!starter || !meta) return null;
  return STARTER_HEADER(meta.title, meta.githubLink) + "\n" + starter(meta.title);
}
