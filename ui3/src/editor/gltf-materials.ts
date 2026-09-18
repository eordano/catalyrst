import { COMPONENT_SCHEMAS, schemaError } from "./authoring-schema";

export type MaterialValue = Record<string, unknown>;
export interface GltfSwap extends MaterialValue { path: string; castShadows?: boolean; material?: MaterialValue }
export interface GltfModifiers extends MaterialValue { modifiers: GltfSwap[] }
export type MaterialKind = "pbr" | "unlit";
export type TextureKind = "texture" | "avatarTexture" | "videoTexture";

export function object(value: unknown): MaterialValue {
  return value && typeof value === "object" && !Array.isArray(value) ? value as MaterialValue : {};
}
export function modifiersValue(value: unknown): GltfModifiers {
  const source = object(value);
  return structuredClone({ ...source, modifiers: Array.isArray(source.modifiers) ? source.modifiers : [] }) as GltfModifiers;
}
export function materialValue(swap: GltfSwap): { kind: MaterialKind; value: MaterialValue } {
  return materialComponentValue(object(swap.material));
}
export function materialComponentValue(value: MaterialValue): { kind: MaterialKind; value: MaterialValue } {
  const union = object(value.material);
  const kind = union.$case === "unlit" || "unlit" in union ? "unlit" : "pbr";
  return { kind, value: object(union[kind]) };
}
export function withMaterial(swap: GltfSwap, kind: MaterialKind, value: MaterialValue): GltfSwap {
  return { ...swap, material: withMaterialComponent(object(swap.material), kind, value) };
}
export function withMaterialComponent(component: MaterialValue, kind: MaterialKind, value: MaterialValue): MaterialValue {
  return { ...component, material: { $case: kind, [kind]: value } };
}
export function newMaterialSwap(): GltfSwap {
  return withMaterial({ path: "", castShadows: true }, "pbr", { metallic: 0.5, roughness: 0.5, castShadows: true });
}
export function textureValue(value: unknown): { kind: TextureKind; value: MaterialValue } {
  const union = object(object(value).tex);
  const kind = "avatarTexture" in union ? "avatarTexture" : "videoTexture" in union ? "videoTexture" : "texture";
  return { kind, value: object(union[kind]) };
}
export function newTexture(kind: TextureKind): MaterialValue {
  const value = kind === "texture" ? { src: "", offset: { x: 0, y: 0 }, tiling: { x: 1, y: 1 } } : kind === "avatarTexture" ? { userId: "" } : { videoPlayerEntity: 0 };
  return { ...value, wrapMode: 0, filterMode: 0 };
}
export function textureAssetPaths(files: { path: string }[]): string[] {
  return files.map(file => file.path).filter(path => /\.(png|jpe?g|webp|gif|avif|ktx2?)$/i.test(path)).sort();
}
export function materialComponentError(value: MaterialValue): string | null {
  const error = schemaError(COMPONENT_SCHEMAS["core::Material"]!.schema, value);
  if (error) return error;
  const material = materialComponentValue(value).value;
  for (const key of ["texture", "alphaTexture", "bumpTexture", "emissiveTexture"]) {
    if (!material[key]) continue;
    const texture = textureValue(material[key]);
    if (texture.kind === "texture") {
      const src = String(texture.value.src ?? "");
      if (!src.trim()) return `Choose a ${key === "texture" ? "base" : key.replace("Texture", "")} texture or remove it.`;
      if (!/^https:\/\//i.test(src) && (/^[a-z]+:|^\/|\\/.test(src) || src.split("/").includes(".."))) return "Use a project texture path or an HTTPS URL.";
    }
    if (texture.kind === "videoTexture" && (!Number.isSafeInteger(texture.value.videoPlayerEntity) || Number(texture.value.videoPlayerEntity) < 0)) return "Enter a valid video entity ID.";
    if (texture.kind === "avatarTexture" && !String(texture.value.userId ?? "").trim()) return "Enter an avatar user ID.";
  }
  return null;
}
export function materialSwapError(value: GltfModifiers): string | null {
  const error = schemaError(COMPONENT_SCHEMAS["core::GltfNodeModifiers"]!.schema, value);
  if (error) return error;
  for (const [index, swap] of value.modifiers.entries()) {
    if (!swap.material) continue;
    const error = materialComponentError(swap.material);
    if (error) return `Swap ${index + 1}: ${error}`;
  }
  return null;
}
