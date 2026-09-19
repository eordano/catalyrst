import { materialComponentError, materialComponentValue, object, type MaterialValue } from "./gltf-materials";

export interface MaterialSelection { entity: string; name: string; value?: MaterialValue }
export interface MaterialChange { entity: string; name: string; before: MaterialValue; value: MaterialValue }
export interface MaterialEdit { paths: string[][]; replaceType?: boolean }
export interface MaterialSelectionDraft {
  selection: MaterialSelection[];
  value: MaterialValue;
  paths: string[][];
  replaceType: boolean;
}
export type AuthorMaterialSelection = (changes: MaterialChange[]) => Promise<void>;

export function selectionDraft(raw: unknown): MaterialSelectionDraft {
  const selection = structuredClone(raw) as MaterialSelection[];
  return { selection, value: structuredClone(selection[0]?.value ?? {}), paths: [], replaceType: false };
}
export function changeSelection(draft: MaterialSelectionDraft, value: MaterialValue, edit: MaterialEdit): MaterialSelectionDraft {
  return { ...draft, value, paths: [...draft.paths, ...edit.paths], replaceType: draft.replaceType || Boolean(edit.replaceType) };
}
export function selectionHasMixedTypes(draft: MaterialSelectionDraft): boolean {
  const kind = materialComponentValue(draft.value).kind;
  return !draft.replaceType && draft.selection.some(item => item.value && materialComponentValue(item.value).kind !== kind);
}
function readPath(value: unknown, path: string[]): unknown {
  return path.reduce<unknown>((current, key) => object(current)[key], value);
}
function writePath(value: MaterialValue, path: string[], replacement: unknown): void {
  const last = path.at(-1)!;
  let current = value;
  for (const key of path.slice(0, -1)) {
    const defaults = key.endsWith("Color") ? { r: 1, g: 1, b: 1, ...(key === "albedoColor" || key === "diffuseColor" ? { a: 1 } : {}) }
      : key === "offset" ? { x: 0, y: 0 } : key === "tiling" ? { x: 1, y: 1 } : {};
    current = current[key] = { ...defaults, ...object(current[key]) };
  }
  if (replacement === undefined) delete current[last];
  else current[last] = structuredClone(replacement);
}
export function selectionChanges(draft: MaterialSelectionDraft): MaterialChange[] {
  const error = selectionError(draft);
  if (error) throw new Error(error);
  return draft.selection.map(item => {
    const before = structuredClone(item.value!);
    const value = structuredClone(before);
    if (draft.replaceType) value.material = structuredClone(draft.value.material);
    else for (const path of draft.paths) {
      const tex = path.indexOf("tex");
      if (tex >= 0) {
        const variant = path[tex + 1]!;
        const union = object(readPath(value, path.slice(0, tex + 1)));
        if (!(variant in union)) throw new Error("Selected textures use different sources. Choose a texture source before editing its fields.");
      }
      writePath(value, path, readPath(draft.value, path));
    }
    const error = materialComponentError(value);
    if (error) throw new Error(`${item.entity}: ${error}`);
    return { entity: item.entity, name: item.name, before, value };
  });
}
export function selectionError(draft: MaterialSelectionDraft): string | null {
  if (draft.selection.length < 2 || draft.selection.length > 100) return "Select between 2 and 100 materials.";
  if (draft.selection.some(item => !item.value || !['Material', 'core::Material'].includes(item.name) || !Number.isSafeInteger(Number(item.entity)) || Number(item.entity) < 512)) return "Every selected entity must have a Material component to edit together.";
  if (new Set(draft.selection.map(item => item.entity)).size !== draft.selection.length) return "Select each entity once.";
  if (selectionHasMixedTypes(draft)) return "These materials have different types. Choose a material type to edit them together.";
  return materialComponentError(draft.value);
}
