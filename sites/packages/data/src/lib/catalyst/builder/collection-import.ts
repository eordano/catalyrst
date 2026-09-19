import { validateModelFile } from "./model-file";
import { unzipSync } from "fflate";
import { z } from "zod";

const MAX_BYTES = 20 * 1024 * 1024;
const shapes = ["urn:decentraland:off-chain:base-avatars:BaseMale", "urn:decentraland:off-chain:base-avatars:BaseFemale"];
const Metadata = z.object({
  name: z.string().min(1).max(64).optional(), description: z.string().max(4096).default(""),
  type: z.enum(["wearable", "emote"]).default("wearable"),
  rarity: z.enum(["unique", "mythic", "exotic", "legendary", "epic", "rare", "uncommon", "common"]).default("common"),
  data: z.record(z.string(), z.unknown()).default({}), thumbnail: z.string().optional(),
});

export type ImportedItem = {
  name: string; description: string; type: "wearable" | "emote"; rarity: string;
  data: Record<string, unknown>; thumbnail: string | null; files: File[];
};

export function validContentPath(name: string): boolean {
  return name.length > 0 && name.length <= 256 && !/[\\\0]/.test(name)
    && name.split("/").every(part => part !== "" && part !== "." && part !== "..");
}

export async function importCollectionItem(file: File): Promise<ImportedItem> {
  if (!file.size || file.size > MAX_BYTES) throw new Error(`${file.name}: choose a file smaller than 20 MB.`);
  let files: File[];
  if (/\.zip$/i.test(file.name)) {
    let size = 0;
    const names = new Set<string>();
    const entries = unzipSync(new Uint8Array(await file.arrayBuffer()), { filter(entry) {
      if (entry.name.endsWith("/") || entry.name.startsWith("__MACOSX/")) return false;
      size += entry.originalSize;
      if (!validContentPath(entry.name) || names.has(entry.name)) throw new Error("ZIP contains an invalid or repeated filename.");
      names.add(entry.name);
      if (names.size > 100 || size > MAX_BYTES) throw new Error("ZIP must contain at most 100 files and expand to at most 20 MB.");
      return true;
    } });
    files = Object.entries(entries).map(([name, bytes]) => new File([Uint8Array.from(bytes)], name));
  } else files = [file];
  if (!files.length || files.some(part => !validContentPath(part.name))) throw new Error("Choose a model or an item ZIP.");
  const wearable = files.find(part => part.name === "wearable.json");
  const emote = files.find(part => part.name === "emote.json");
  if (wearable && emote) throw new Error("Import wearables and emotes as separate ZIP files.");
  const manifest = wearable ?? emote;
  const config = manifest ? JSON.parse(await manifest.text()) : {};
  const metadata = Metadata.parse(emote ? {
    ...config, type: "emote", data: { category: config.category ?? "dance", loop: config.play_mode === "loop", tags: config.tags ?? [] },
  } : config);
  const models = files.filter(part => /\.(glb|gltf)$/i.test(part.name));
  const pngs = files.filter(part => /\.png$/i.test(part.name));
  let representations: unknown = metadata.data.representations;
  if (!representations) {
    const candidates = models.length ? models : pngs.filter(part => part.name !== "thumbnail.png" && !/_(?:mask|expressions|expressions_mask)\.png$/i.test(part.name));
    const shapeFor = (name: string) => /(?:^|\/)female\//i.test(name) ? shapes[1] : /(?:^|\/)male\//i.test(name) ? shapes[0] : undefined;
    if (!candidates.length || (candidates.length > 1 && (candidates.length !== 2 || candidates.some(part => !shapeFor(part.name)) || shapeFor(candidates[0].name) === shapeFor(candidates[1].name)))) {
      throw new Error(`${file.name}: include one model, male/female folders, or wearable.json describing the representations.`);
    }
    representations = candidates.map(candidate => ({ bodyShapes: shapeFor(candidate.name) ? [shapeFor(candidate.name)] : shapes, mainFile: candidate.name, contents: files.filter(part => part !== manifest).map(part => part.name), overrideHides: [], overrideReplaces: [] }));
  }
  const parsed = z.array(z.object({
    mainFile: z.string(), contents: z.array(z.string()), bodyShapes: z.array(z.string()).min(1),
  }).passthrough()).min(1).parse(representations);
  const available = new Set(files.map(part => part.name));
  for (const rep of parsed) {
    if (!available.has(rep.mainFile) || !rep.contents.includes(rep.mainFile) || rep.contents.some(name => !available.has(name))) {
      throw new Error(`${file.name}: a representation references a missing file.`);
    }
    if (rep.bodyShapes.some(shape => !shapes.includes(shape))) throw new Error("Choose a supported avatar body shape.");
  }
  for (const model of models) await validateModelFile(model, available);
  for (const image of pngs) {
    const signature = new Uint8Array(await image.slice(0, 8).arrayBuffer());
    if (signature.length !== 8 || signature.some((byte, index) => byte !== [137, 80, 78, 71, 13, 10, 26, 10][index])) throw new Error(`${image.name}: invalid PNG image.`);
  }
  const thumbnail = metadata.thumbnail ?? (available.has("thumbnail.png") ? "thumbnail.png" : null);
  if (thumbnail && !available.has(thumbnail)) throw new Error("The item thumbnail is missing from the ZIP.");
  return {
    name: metadata.name ?? file.name.replace(/\.[^.]+$/, "").slice(0, 64),
    description: metadata.description, type: metadata.type, rarity: metadata.rarity,
    data: { ...metadata.data, representations: parsed }, thumbnail, files,
  };
}
