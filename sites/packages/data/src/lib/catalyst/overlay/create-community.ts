import { z } from "zod";

export type CommunityPrivacy = "public" | "private";
export type CommunityVisibility = "all" | "unlisted";

export const COMMUNITY_BOUNDS = {
  nameMax: 30,
  descriptionMax: 500,
  thumbMinBytes: 1024,
  thumbMaxBytes: 500 * 1024,
} as const;

export type OwnedPlace = {
  id: string;
  title: string;
  coords: string;
  kind: "land" | "world";
};

export type CommunityDraft = {
  name: string;
  description: string;
  privacy: CommunityPrivacy;
  visibility: CommunityVisibility;
  hasThumbnail: boolean;
  thumbnailPreviewUrl: string | null;
  placeIds: string[];
};

export function emptyDraft(): CommunityDraft {
  return {
    name: "",
    description: "",
    privacy: "public",
    visibility: "all",
    hasThumbnail: false,
    thumbnailPreviewUrl: null,
    placeIds: [],
  };
}

export type CreateCommunityBody = {
  name: string;
  description: string;
  private: boolean;
  unlisted: boolean;
  flags: string[];
};

export function toCreateCommunityBody(draft: CommunityDraft): CreateCommunityBody {
  return {
    name: draft.name.trim(),
    description: draft.description.trim(),
    private: draft.privacy === "private",
    unlisted: draft.visibility === "unlisted",
    flags: [],
  };
}

export type DraftIssues = Partial<Record<keyof CommunityDraft, string>>;

export function validateStep(step: string, draft: CommunityDraft): DraftIssues {
  const issues: DraftIssues = {};
  if (step === "basics") {
    const name = draft.name.trim();
    if (!name) issues.name = "Community name is required";
    else if (name.length > COMMUNITY_BOUNDS.nameMax)
      issues.name = `Name must be ${COMMUNITY_BOUNDS.nameMax} characters or fewer`;
    if (hasForbiddenControl(draft.name))
      issues.name = "Name contains forbidden control characters";
    const desc = draft.description.trim();
    if (!desc) issues.description = "Description is required";
    else if (desc.length > COMMUNITY_BOUNDS.descriptionMax)
      issues.description = `Description must be ${COMMUNITY_BOUNDS.descriptionMax} characters or fewer`;
  }
  return issues;
}

export function isStepValid(step: string, draft: CommunityDraft): boolean {
  return Object.keys(validateStep(step, draft)).length === 0;
}

function hasForbiddenControl(s: string): boolean {
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c === 0x09 || c === 0x0a || c === 0x0d) continue;
    if (c <= 0x1f || (c >= 0x7f && c <= 0x9f)) return true;
  }
  return false;
}

export const CreateResultSchema = z.object({
  id: z.string(),
  name: z.string(),
  privacy: z.enum(["public", "private"]),
  visibility: z.enum(["all", "unlisted"]),
});
export type CreateResult = z.infer<typeof CreateResultSchema>;

export async function simulateCreateCommunity(
  draft: CommunityDraft,
  opts: { signal?: AbortSignal; delayMs?: number } = {},
): Promise<CreateResult> {
  const body = toCreateCommunityBody(draft);
  if (!body.name) throw new Error("name is required");
  if (!body.description) throw new Error("description is required");

  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, opts.delayMs ?? 500);
    opts.signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });

  const id = simCommunityIdHex(body.name);
  return {
    id,
    name: body.name,
    privacy: draft.privacy,
    visibility: draft.visibility,
  };
}

function simCommunityIdHex(name: string): string {
  const seed = `sim-community:${name}`;
  let out = "";
  let h = 0x811c9dc5;
  for (let i = 0; out.length < 64; i++) {
    const ch = name.charCodeAt(i % Math.max(1, name.length)) ^ (seed.length + i);
    h = Math.imul(h ^ ch, 0x01000193) >>> 0;
    out += h.toString(16).padStart(8, "0");
  }
  return out.slice(0, 64);
}
