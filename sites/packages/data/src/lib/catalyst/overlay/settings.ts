import { z } from "zod";

// Single source shared with the ui3 SPA panel: the catalog data lives in ui3
// (both surfaces render it through the same presentational component); this
// module remains the validating loader for the sites surface.
import settingsFixture from "@ui/data/settings/settings-catalog.data.json";

const ModuleSchema = z
  .object({
    key: z.string().min(1),
    kind: z.enum(["toggle", "slider", "dropdown"]),
    title: z.string().min(1),
    setting: z.string().min(1).optional(),
    fullscreen: z.boolean().optional(),
    options: z.array(z.string()).optional(),
    default: z.number().optional(),
    min: z.number().optional(),
    max: z.number().optional(),
    unit: z.string().optional(),
  })
  .passthrough()
  .superRefine((m, ctx) => {
    if (!m.fullscreen && !m.setting) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: `module ${m.key} names no engine setting and is not the fullscreen special`,
      });
    }
    if (m.setting && m.default === undefined) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: `module ${m.key} needs a seed default for first paint`,
      });
    }
  });

const GroupSchema = z
  .object({
    title: z.string(),
    modules: z.array(ModuleSchema),
  })
  .passthrough();

const TabSchema = z.object({ id: z.string().min(1), label: z.string().min(1) });

const SettingsFixtureSchema = z
  .object({
    tabs: z.array(TabSchema).min(1),
    sections: z.record(z.string(), z.array(GroupSchema)),
  })
  .passthrough();

export type SettingModule = z.infer<typeof ModuleSchema>;
export type SettingGroup = z.infer<typeof GroupSchema>;
export type SettingsTab = z.infer<typeof TabSchema>;

export type SettingsCatalog = {
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

export function parseSettingsFixture(input: unknown): SettingsCatalog {
  const parsed = SettingsFixtureSchema.parse(input);
  return {
    tabs: parsed.tabs,
    sections: parsed.sections as Record<string, SettingGroup[]>,
  };
}

export function loadSettingsCatalog(): SettingsCatalog {
  if (cached) return cached;
  cached = parseSettingsFixture(settingsFixture);
  return cached;
}

export function groupsForTab(catalog: SettingsCatalog, tab: TabId): SettingGroup[] {
  return catalog.sections[tab] ?? [];
}

export function defaultValues(catalog: SettingsCatalog): Record<string, number> {
  const out: Record<string, number> = {};
  for (const groups of Object.values(catalog.sections)) {
    for (const g of groups) {
      for (const m of g.modules) {
        if (m.setting && m.default !== undefined) out[m.setting] = m.default;
      }
    }
  }
  return out;
}
