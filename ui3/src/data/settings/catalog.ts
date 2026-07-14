import catalogJson from "./settings-catalog.data.json";

export type SettingModuleKind = "toggle" | "slider" | "dropdown";

export type SettingModule = {
  key: string;
  kind: SettingModuleKind;
  title: string;
  setting?: string;
  fullscreen?: boolean;
  options?: string[];
  default?: number;
  min?: number;
  max?: number;
  unit?: string;
};

export type SettingGroup = { title: string; modules: SettingModule[] };
export type SettingsTab = { id: string; label: string };

export type SettingsCatalog = {
  tabs: SettingsTab[];
  sections: Record<string, SettingGroup[]>;
};

// The one catalog both surfaces render: the sites data layer re-imports this
// JSON and zod-validates it; here the cast is enough because that gate already
// covers the same file.
export const SETTINGS_CATALOG = catalogJson as unknown as SettingsCatalog;

export function groupsForTab(catalog: SettingsCatalog, tab: string): SettingGroup[] {
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
