
export const CREATE_ENTRY_ARMS = [
  "download-hub",
  "builder-or-download",
  "hub-or-download",
  "capability-routed",
] as const;

export type CreateEntryArm = (typeof CREATE_ENTRY_ARMS)[number];

export const CREATE_ENTRY_STORIES = {
  "entry-preview": "create_entry_preview",
  "download-hub": "create_download_hub",
  "builder-or-download": "create_builder_or_download",
  "hub-or-download": "create_hub_or_download",
  "capability-routed": "create_capability_routed",
} as const;

export type CreateEntryStoryName = keyof typeof CREATE_ENTRY_STORIES;

export type CreateEntryConfig = {
  story: string;
  experimentKey: string;
  variant: string;
  entry: CreateEntryArm | null;
  webHubIfCapable: boolean;
};

export function activeCreateExperiment(
  raw: string | undefined | null,
): CreateEntryStoryName | null {
  const v = raw?.trim();
  if (!v) return null;
  if (v in CREATE_ENTRY_STORIES) return v as CreateEntryStoryName;
  for (const [dir, key] of Object.entries(CREATE_ENTRY_STORIES)) {
    if (key === v) return dir as CreateEntryStoryName;
  }
  return null;
}

export function entryFromFlags(flags: Record<string, unknown>): CreateEntryArm | null {
  const entry = flags["entry"];
  if (typeof entry !== "string") return null;
  return (CREATE_ENTRY_ARMS as readonly string[]).includes(entry)
    ? (entry as CreateEntryArm)
    : null;
}

export function webHubIfCapable(flags: Record<string, unknown>): boolean {
  return flags["webHubIfCapable"] === true;
}

export function armOverride(
  url: URL,
  variants: ReadonlyArray<{ id: string }>,
): string | undefined {
  const raw = url.searchParams.get("arm")?.trim();
  if (!raw) return undefined;
  return variants.some((v) => v.id === raw) ? raw : undefined;
}

export const CREATE_ENTRY_TARGETS = {
  builder: "/create/wearables/item-editor?from=create-entry",
  webHub: "/creator-hub/scene-editor?new=1&from=create-entry",
  download: "/landings/creator-hub-download",
} as const;
