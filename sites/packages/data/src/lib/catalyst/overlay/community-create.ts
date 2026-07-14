import {
  COMMUNITY_BOUNDS,
  CreateResultSchema,
  simulateCreateCommunity,
  toCreateCommunityBody,
  type CommunityPrivacy,
  type CommunityVisibility,
  type CreateCommunityBody,
  type CreateResult,
} from "./create-community";

export {
  COMMUNITY_BOUNDS,
  CreateResultSchema,
  toCreateCommunityBody,
  type CommunityPrivacy,
  type CommunityVisibility,
  type CreateCommunityBody,
  type CreateResult,
};

export type CommunityDraft = {
  name: string;
  description: string;
  privacy: CommunityPrivacy;
  visibility: CommunityVisibility;
  hasThumbnail: boolean;
  policyAck: boolean;
};

export function emptyDraft(): CommunityDraft {
  return {
    name: "",
    description: "",
    privacy: "public",
    visibility: "all",
    hasThumbnail: false,
    policyAck: false,
  };
}

export type DraftIssues = Partial<Record<keyof CommunityDraft, string>>;

export function validateStep(step: string, draft: CommunityDraft): DraftIssues {
  const issues: DraftIssues = {};
  if (step === "details") {
    const name = draft.name.trim();
    if (!name) issues.name = "Community name is required";
    else if (name.length > COMMUNITY_BOUNDS.nameMax)
      issues.name = `Name must be ${COMMUNITY_BOUNDS.nameMax} characters or fewer`;
    else if (hasForbiddenControl(draft.name))
      issues.name = "Name contains forbidden control characters";
  }
  if (step === "review") {
    if (!draft.policyAck)
      issues.policyAck = "You must acknowledge the Content Policy to continue";
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

export async function simulateCreate(
  draft: CommunityDraft,
  opts: { signal?: AbortSignal; delayMs?: number } = {},
): Promise<CreateResult> {
  return simulateCreateCommunity(
    {
      name: draft.name,
      description: draft.description || `${draft.name.trim()} community`,
      privacy: draft.privacy,
      visibility: draft.visibility,
      hasThumbnail: draft.hasThumbnail,
      thumbnailPreviewUrl: null,
      placeIds: [],
    },
    opts,
  );
}

export type NameGate = {
  hasName: boolean;
  ownedNames: string[];
};

export function noName(): NameGate {
  return { hasName: false, ownedNames: [] };
}

export function ownsName(names: string[] = ["your-name"]): NameGate {
  return { hasName: names.length > 0, ownedNames: names };
}
