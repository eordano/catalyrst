import { z } from "zod";

import settingsFixture from "./settings-catalog.data.json";

const ModuleSchema = z
  .object({
    key: z.string().min(1),
    kind: z.enum(["toggle", "slider", "dropdown"]),
    title: z.string().min(1),
    options: z.array(z.string()).optional(),
    default: z.union([z.string(), z.number(), z.boolean()]),
    min: z.number().optional(),
    max: z.number().optional(),
    unit: z.string().optional(),
  })
  .passthrough();

const GroupSchema = z
  .object({
    title: z.string().min(1),
    modules: z.array(ModuleSchema),
  })
  .passthrough();

const TabSchema = z.object({ id: z.string().min(1), label: z.string().min(1) });

const SettingsFixtureSchema = z
  .object({
    storageKey: z.string().min(1),
    tabs: z.array(TabSchema).min(1),
    sections: z.record(z.string(), z.array(GroupSchema)),
  })
  .passthrough();

export type SettingModule = z.infer<typeof ModuleSchema>;
export type SettingGroup = z.infer<typeof GroupSchema>;
export type SettingsTab = z.infer<typeof TabSchema>;

export type SettingsCatalog = {
  storageKey: string;
  tabs: SettingsTab[];
  sections: Record<string, SettingGroup[]>;
};

export const TAB_IDS = ["graphics", "sounds", "controls", "chat"] as const;
export type TabId = (typeof TAB_IDS)[number];

export const DEFAULT_TAB: TabId = "graphics";

export function parseTab(raw: string | null | undefined): TabId {
  const t = (raw ?? "").trim().toLowerCase();
  return (TAB_IDS as readonly string[]).includes(t) ? (t as TabId) : DEFAULT_TAB;
}

let cached: SettingsCatalog | null = null;

export function loadSettingsCatalog(): SettingsCatalog {
  if (cached) return cached;
  const parsed = SettingsFixtureSchema.parse(settingsFixture);
  cached = {
    storageKey: parsed.storageKey,
    tabs: parsed.tabs,
    sections: parsed.sections as Record<string, SettingGroup[]>,
  };
  return cached;
}

export function groupsForTab(catalog: SettingsCatalog, tab: TabId): SettingGroup[] {
  return catalog.sections[tab] ?? [];
}

export function defaultValues(catalog: SettingsCatalog): Record<string, string | number | boolean> {
  const out: Record<string, string | number | boolean> = {};
  for (const groups of Object.values(catalog.sections)) {
    for (const g of groups) {
      for (const m of g.modules) out[m.key] = m.default;
    }
  }
  return out;
}
