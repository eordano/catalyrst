import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { submitProposal } from "./submit-client";
import type { AuthIdentity } from "../../auth/types";
import { MembersEnvelopeSchema } from "../generated-schemas/governance";
import { governanceApiBase } from "./api-base";
import { validateCoAuthors as sharedValidateCoAuthors, type FieldErrors } from "./co-authors";
import { ETH_ADDRESS_RE } from "../format/address";

export const HIRING_REQUESTS = ["add", "remove"] as const;
export type HiringRequest = (typeof HIRING_REQUESTS)[number];

export const HIRING_TYPE: Record<HiringRequest, string> = {
  add: "hiring_add",
  remove: "hiring_remove",
};

export function toHiringRequest(
  raw: string | null | undefined,
): HiringRequest | null {
  const v = raw?.trim().toLowerCase();
  return v === "add" || v === "remove" ? v : null;
}

const RequestCopySchema = z.object({
  title: z.string(),
  description: z.string(),
  addressTitle: z.string(),
  addressDescription: z.string(),
  reasonsTitle: z.string(),
  reasonsDescription: z.string(),
  evidenceTitle: z.string(),
  evidenceDescription: z.string(),
});

const MemberSchema = z.object({
  address: z.string(),
  label: z.string(),
  name: z.string(),
  hue: z.number(),
});

const CommitteeSchema = z.object({
  name: z.string(),
  size: z.number(),
  openSlots: z.boolean(),
  members: z.array(MemberSchema),
});

const SampleSchema = z.object({
  committee: z.string(),
  address: z.string(),
  reasons: z.string(),
  evidence: z.string(),
});

const SubmitHiringSchema = z.object({
  schema: z.object({
    reasons: z.object({ min: z.number(), max: z.number() }),
    evidence: z.object({ min: z.number(), max: z.number() }),
    name: z.object({ min: z.number(), max: z.number() }).optional(),
    coAuthors: z.object({ max: z.number(), addressLength: z.number() }),
    submissionThresholdVp: z.number(),
  }),
  account: z.object({
    address: z.string(),
    label: z.string(),
    votingPower: z.number(),
  }),
  copy: z.object({
    category: z.object({ title: z.string(), description: z.string() }),
    add: RequestCopySchema,
    remove: RequestCopySchema,
    shared: z.object({
      targetTitle: z.string(),
      targetPlaceholder: z.string(),
      targetDescription: z.string(),
      addressPlaceholder: z.string(),
      memberPlaceholder: z.string(),
      coAuthorsTitle: z.string(),
      coAuthorsDescription: z.string(),
      submitLabel: z.string(),
    }),
    errors: z.object({
      addressInvalid: z.string(),
      committeeRequired: z.string(),
      reasonsRequired: z.string(),
      reasonsMinLength: z.string(),
      reasonsMaxLength: z.string(),
      evidenceRequired: z.string(),
      evidenceMinLength: z.string(),
      evidenceMaxLength: z.string(),
    }),
    submitError: z.string(),
    success: z.object({ title: z.string(), lead: z.string(), note: z.string() }),
  }),
  committees: z.array(CommitteeSchema),
  samples: z.object({ add: SampleSchema, remove: SampleSchema }),
});

export type RequestCopy = z.infer<typeof RequestCopySchema>;
export type CommitteeMember = z.infer<typeof MemberSchema>;
export type Committee = z.infer<typeof CommitteeSchema>;
export type HiringSample = z.infer<typeof SampleSchema>;
export type SubmitHiringData = z.infer<typeof SubmitHiringSchema>;

const STATIC_CONFIG: SubmitHiringData = {
  schema: {
    reasons: { min: 20, max: 3000 },
    evidence: { min: 20, max: 3000 },
    name: { min: 2, max: 15 },
    coAuthors: { max: 5, addressLength: 42 },
    submissionThresholdVp: 2500,
  },
  account: {
    address: "",
    label: "",
    votingPower: 0,
  },
  copy: {
    category: {
      title: "Hiring",
      description: "Request a Community member to be added or removed from a Committee",
    },
    add: {
      title: "Add Committee Member",
      description:
        "Being part of a Committee is great responsibility inside the DAO. Check with whom you are proposing that they agree to be postulated before creating this proposal.",
      addressTitle: "Wallet address",
      addressDescription:
        "Please copy the address of the proposed member. Check with them which address should you provide",
      reasonsTitle: "Reasons for adding",
      reasonsDescription:
        "Explain why you think this person should be added to the Committee.",
      evidenceTitle: "Evidence",
      evidenceDescription:
        "Be as objective and detailed as possible. List their qualifications and achievements.",
    },
    remove: {
      title: "Remove Committee Member",
      description:
        "Use this type of Proposal wisely. Before going through this way, talk with the person first.",
      addressTitle: "Committee member",
      addressDescription: "",
      reasonsTitle: "Reasons for removing",
      reasonsDescription:
        "Explain why this person should be removed from the Committee.",
      evidenceTitle: "Evidence",
      evidenceDescription:
        "Be as objective and detailed as possible. Provide only publicly available information.",
    },
    shared: {
      targetTitle: "Target Committee",
      targetPlaceholder: "Select a committee",
      targetDescription: "Only those with available positions are listed",
      addressPlaceholder: "Enter their address",
      memberPlaceholder: "Select a member",
      coAuthorsTitle: "Co-authors",
      coAuthorsDescription:
        "If you co-authored this proposal with someone else, you can add their wallet addresses to acknowledge their work.",
      submitLabel: "Submit proposal",
    },
    errors: {
      addressInvalid: "Address is invalid",
      committeeRequired: "Select a committee",
      reasonsRequired: "Reasons are required",
      reasonsMinLength: "Reasons field is too short",
      reasonsMaxLength: "Reasons field is too long",
      evidenceRequired: "Evidence is required",
      evidenceMinLength: "Evidence field is too short",
      evidenceMaxLength: "Evidence field is too long",
    },
    submitError:
      "There was an error while trying to create the proposal, please try again later.",
    success: {
      title: "Proposal submitted",
      lead: "Your Hiring proposal is now ready for the DAO to review.",
      note: "This proposal was created in a simulation \u{2014} no on-chain transaction or Snapshot proposal was submitted.",
    },
  },
  committees: [
    { name: "DAO Council", size: 5, openSlots: true, members: [] },
    { name: "Wearable Curation Committee", size: 5, openSlots: true, members: [] },
    { name: "Security Advisory Board", size: 5, openSlots: false, members: [] },
  ],
  samples: {
    add: {
      committee: "DAO Council",
      address: "0x06012c8cf97bead5deae237070f9587f8e7a266d",
      reasons:
        "This contributor has consistently shown up for the DAO across multiple seasons and would strengthen the Council's day-to-day operations.",
      evidence:
        "They authored three accepted governance proposals and have a public track record of on-time milestone delivery.",
    },
    remove: {
      committee: "Security Advisory Board",
      address: "0x12a4b5c6d7e8f90112233445566778899a009f0c",
      reasons:
        "This member has been inactive for two consecutive seasons and is no longer reachable through the committee's public channels.",
      evidence:
        "Signing history on the multisig shows zero participation since the last term.",
    },
  },
};

export function getSubmitHiringData(): SubmitHiringData {
  const parsed = SubmitHiringSchema.safeParse(STATIC_CONFIG);
  return parsed.success ? parsed.data : STATIC_CONFIG;
}

export type HiringSubmitContext = {
  request: HiringRequest;
  hiringType: string;
  copy: RequestCopy;
  shared: SubmitHiringData["copy"]["shared"];
  errors: SubmitHiringData["copy"]["errors"];
  submitError: string;
  success: SubmitHiringData["copy"]["success"];
  schema: SubmitHiringData["schema"];
  account: SubmitHiringData["account"];
  committees: Committee[];
  sample: HiringSample;
  membersLive: boolean;
};

function buildContext(
  request: HiringRequest,
  committees: Committee[],
  membersLive: boolean,
): HiringSubmitContext {
  const data = STATIC_CONFIG;
  const filtered =
    request === "add" ? committees.filter((c) => c.openSlots) : committees;
  return {
    request,
    hiringType: HIRING_TYPE[request],
    copy: data.copy[request],
    shared: data.copy.shared,
    errors: data.copy.errors,
    submitError: data.copy.submitError,
    success: data.copy.success,
    schema: data.schema,
    account: data.account,
    committees: filtered,
    sample: data.samples[request],
    membersLive,
  };
}

const ROLE_TO_COMMITTEE: Record<string, string> = {
  council: "DAO Council",
};

function shortenAddress(addr: string): string {
  const a = addr.trim();
  return a.length >= 10 ? `${a.slice(0, 6)}\u{2026}${a.slice(-4)}` : a;
}

function hueFromAddress(addr: string): number {
  let h = 0;
  const s = addr.toLowerCase();
  for (let i = 2; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) % 360;
  return h;
}

async function resolveProfileName(
  address: string,
  opts: GetOptions,
): Promise<string | null> {
  try {
    const raw = await getJSON<{ avatars?: Array<{ name?: unknown }> }>(
      `/lambdas/profiles/${encodeURIComponent(address)}`,
      opts,
    );
    const name = raw?.avatars?.[0]?.name;
    return typeof name === "string" && name.trim() ? name.trim() : null;
  } catch {
    return null;
  }
}

export type CommitteeMembership = Record<string, CommitteeMember[]>;

export async function fetchCommitteeMembership(
  opts: GetOptions = {},
): Promise<CommitteeMembership> {
  const envelope = await getJSON<unknown>("/members", {
    ...opts,
    base: governanceApiBase(opts.base),
  });
  const parsed = MembersEnvelopeSchema.safeParse(envelope);
  if (!parsed.success) return {};

  const byCommittee: Record<string, string[]> = {};
  for (const row of parsed.data.data) {
    const committee = ROLE_TO_COMMITTEE[row.role];
    if (!committee) continue;
    (byCommittee[committee] ??= []).push(row.address);
  }

  const out: CommitteeMembership = {};
  await Promise.all(
    Object.entries(byCommittee).map(async ([committee, addresses]) => {
      out[committee] = await Promise.all(
        addresses.map(async (address) => {
          const name = await resolveProfileName(address, opts);
          const label = shortenAddress(address);
          return {
            address,
            label,
            name: name ?? label,
            hue: hueFromAddress(address),
          };
        }),
      );
    }),
  );
  return out;
}

export async function loadHiringSubmitContext(
  request: HiringRequest,
  opts: GetOptions = {},
): Promise<HiringSubmitContext> {
  try {
    const membership = await fetchCommitteeMembership(opts);
    const merged = STATIC_CONFIG.committees.map((c) => {
      const live = membership[c.name];
      return live && live.length > 0 ? { ...c, members: live } : c;
    });
    return buildContext(request, merged, true);
  } catch {
    return buildContext(request, STATIC_CONFIG.committees, false);
  }
}

export function membersOf(committees: Committee[], committeeName: string): CommitteeMember[] {
  return committees.find((c) => c.name === committeeName)?.members ?? [];
}

export type { FieldErrors };

const SCHEMA = STATIC_CONFIG.schema;

export function isEthAddress(addr: string): boolean {
  return ETH_ADDRESS_RE.test(addr.trim());
}

export function validateTarget(args: {
  request: HiringRequest;
  committee: string;
  address: string;
  errorCopy: SubmitHiringData["copy"]["errors"];
}): FieldErrors {
  const { committee, address, errorCopy } = args;
  const errors: FieldErrors = {};
  if (!committee.trim()) errors.committee = errorCopy.committeeRequired;
  if (!isEthAddress(address)) errors.address = errorCopy.addressInvalid;
  return errors;
}

export function validateReasons(args: {
  reasons: string;
  evidence: string;
  errorCopy: SubmitHiringData["copy"]["errors"];
}): FieldErrors {
  const { reasons, evidence, errorCopy } = args;
  const errors: FieldErrors = {};

  const r = reasons.trim().length;
  if (r === 0) errors.reasons = errorCopy.reasonsRequired;
  else if (r < SCHEMA.reasons.min) errors.reasons = errorCopy.reasonsMinLength;
  else if (r > SCHEMA.reasons.max) errors.reasons = errorCopy.reasonsMaxLength;

  const e = evidence.trim().length;
  if (e === 0) errors.evidence = errorCopy.evidenceRequired;
  else if (e < SCHEMA.evidence.min) errors.evidence = errorCopy.evidenceMinLength;
  else if (e > SCHEMA.evidence.max) errors.evidence = errorCopy.evidenceMaxLength;

  return errors;
}

export function validateCoAuthors(coAuthors: string[]): FieldErrors {
  return sharedValidateCoAuthors(coAuthors, SCHEMA.coAuthors.max);
}

export type NewProposalHiring = {
  request: HiringRequest;
  type: string;
  committee: string;
  address: string;
  reasons: string;
  evidence: string;
  coAuthors: string[];
};

export type CreatedProposal = {
  id: string;
  type: string;
  request: HiringRequest;
};

export function buildProposalPayload(
  request: HiringRequest,
  input: {
    committee: string;
    address: string;
    reasons: string;
    evidence: string;
    coAuthors: string[];
  },
): NewProposalHiring {
  return {
    request,
    type: HIRING_TYPE[request],
    committee: input.committee,
    address: input.address.trim(),
    reasons: input.reasons.trim(),
    evidence: input.evidence.trim(),
    coAuthors: input.coAuthors.map((a) => a.trim()).filter(Boolean),
  };
}

export type CreateProposalFn = (args: {
  payload: NewProposalHiring;
  signal?: AbortSignal;
}) => Promise<CreatedProposal>;

const SUBMIT_UNAVAILABLE =
  "hiring proposal submission unavailable: DAO governance signer not configured";

export const failClosedCreateProposal: CreateProposalFn = async () => {
  throw new Error(SUBMIT_UNAVAILABLE);
};

export function buildCreateProposal(
  identity: AuthIdentity | null,
): CreateProposalFn {
  return async ({ payload, signal }) => {
    const created = await submitProposal({
      identity,
      kind: "hiring",
      body: payload,
      unavailable: SUBMIT_UNAVAILABLE,
      signal,
    });
    return {
      id: created.id,
      type: created.type ?? payload.type,
      request: payload.request,
    };
  };
}
