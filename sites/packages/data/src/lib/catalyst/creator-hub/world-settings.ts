import { z } from "zod";
import type { WorldSettingsValues } from "@ui/creatorhub/components/ChWorldSettingsTabbedSections";
import type { AuthIdentity } from "../../auth/types";
import { signedFetch } from "../../auth/signer";
import { getJSON, worldsBase, CatalystError, type GetOptions } from "../client";

const WireSettings = z.object({
  title: z.string().nullable(),
  description: z.string().nullable(),
  categories: z.array(z.string()).nullable(),
  spawn_coordinates: z.string().nullable(),
  skybox_time: z.number().nullable(),
  single_player: z.boolean(),
  show_in_places: z.boolean(),
  thumbnail_hash: z.string().nullable(),
});

function decode(raw: unknown, base: string): WorldSettingsValues {
  const value = WireSettings.parse(raw);
  return {
    title: value.title ?? "",
    description: value.description ?? "",
    categories: value.categories ?? [],
    spawnCoordinates: value.spawn_coordinates ?? "0,0",
    skyboxTime: value.skybox_time,
    singlePlayer: value.single_player,
    showInPlaces: value.show_in_places,
    thumbnailUrl: value.thumbnail_hash ? `${base}/contents/${encodeURIComponent(value.thumbnail_hash)}` : null,
  };
}

export async function loadWorldSettings(world: string, opts: GetOptions = {}): Promise<WorldSettingsValues> {
  const base = worldsBase(opts.base);
  return decode(await getJSON(`/world/${encodeURIComponent(world)}/settings`, { ...opts, base }), base);
}

const fieldNames = {
  title: "title", description: "description", categories: "categories",
  spawnCoordinates: "spawn_coordinates", skyboxTime: "skybox_time",
  singlePlayer: "single_player", showInPlaces: "show_in_places",
} as const;

export type WorldSettingsChanges = Partial<Omit<WorldSettingsValues, "thumbnailUrl">>;

export function settingsForm(changes: WorldSettingsChanges, thumbnail?: File): FormData {
  const form = new FormData();
  for (const key of ["title", "description"] as const) {
    const value = changes[key];
    const max = key === "title" ? 100 : 1000;
    if (value !== undefined && (value.trim().length < 3 || value.trim().length > max)) {
      throw new Error(`${key === "title" ? "Title" : "Description"} must be between 3 and ${max} characters.`);
    }
  }
  if (changes.spawnCoordinates !== undefined) {
    const parts = changes.spawnCoordinates.split(",");
    if (parts.length !== 2 || parts.some(part => !/^-?\d+$/.test(part.trim()) || Math.abs(Number(part)) > 150)) {
      throw new Error("Spawn coordinates must be whole numbers between -150 and 150.");
    }
  }
  if (changes.skyboxTime != null && (!Number.isInteger(changes.skyboxTime) || changes.skyboxTime < 0 || changes.skyboxTime >= 86400)) {
    throw new Error("Choose a valid time of day.");
  }
  if (changes.categories && changes.categories.length > 20) throw new Error("Choose at most 20 categories.");
  for (const key of Object.keys(fieldNames) as (keyof typeof fieldNames)[]) {
    const value = changes[key];
    if (value === undefined) continue;
    if (Array.isArray(value)) {
      for (const category of value.length ? value : ["null"]) form.append(fieldNames[key], category);
    } else {
      form.set(fieldNames[key], value === null ? "null" : String(value));
    }
  }
  if (thumbnail) {
    if (thumbnail.size > 1024 * 1024) throw new Error("Choose an image smaller than 1 MB.");
    if (!["image/png", "image/jpeg", "image/gif", "image/webp"].includes(thumbnail.type)) {
      throw new Error("Choose a PNG, JPEG, GIF or WebP image.");
    }
    form.set("thumbnail", thumbnail);
  }
  return form;
}

export async function saveWorldSettings(world: string, changes: WorldSettingsChanges, opts: {
  identity: AuthIdentity; thumbnail?: File; signal?: AbortSignal; base?: string;
}): Promise<WorldSettingsValues> {
  const base = worldsBase(opts.base);
  const url = `${base}/world/${encodeURIComponent(world)}/settings`;
  const response = await signedFetch(opts.identity, url, {
    method: "PUT", body: settingsForm(changes, opts.thumbnail), signal: opts.signal,
  });
  if (!response.ok) {
    const detail = await response.json().catch(() => null) as { message?: string; error?: string } | null;
    throw new CatalystError(detail?.message ?? detail?.error ?? `Could not save World settings (${response.status}).`, url, response.status);
  }
  const raw = await response.json() as { settings: unknown };
  return decode(raw.settings, base);
}
