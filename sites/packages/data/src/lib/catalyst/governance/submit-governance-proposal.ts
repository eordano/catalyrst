import { z } from "zod";

import { getJSON } from "../client";
import { submitProposal } from "./submit-client";
import type { AuthIdentity } from "../../auth/types";
import type { ListEnvelope as RsListEnvelope } from "@ui/generated/catalyst/governance/ListEnvelope";
import {
  ProposalRowSchema,
  ProposalsEnvelopeSchema,
} from "../generated-schemas/governance";
import { governanceApiBase } from "./api-base";
import { validateCoAuthors as sharedValidateCoAuthors, type FieldErrors } from "./co-authors";

export const GOVERNANCE_SCHEMA = {
  vpThreshold: 2500,
  title: { min: 5, max: 80 },
  bodies: [
    { name: "summary", label: "Summary", sublabel: "One sentence summarizing the proposal.", min: 20, max: 250 },
    { name: "abstract", label: "Abstract", sublabel: "Two to three sentence overview of the proposal, specifying its motivation and outcomes.", min: 1, max: 3500 },
    { name: "motivation", label: "Motivation", sublabel: "Detailed description of the reason why the proposal is necessary/relevant, i.e. what is the problem?", min: 20, max: 3500 },
    { name: "specification", label: "Specification", sublabel: "Detailed description of the proposed policy", min: 20, max: 3500 },
    { name: "impacts", label: "Impacts", sublabel: "Detailed assessment of potential impacts, citing your methods, data sources, or line of reasoning.", min: 20, max: 3500 },
    { name: "implementation_pathways", label: "Implementation Pathways", sublabel: "Detailed description of concrete steps that can be taken to implement the proposal.", min: 20, max: 3500 },
    { name: "conclusion", label: "Conclusion", sublabel: "Closing statement encompassing the motivation or problem, proposed solution, and its intended impact/outcome.", min: 20, max: 3500 },
  ],
  coAuthorsMax: 5,
} as const;

export const BODY_NAMES = GOVERNANCE_SCHEMA.bodies.map((b) => b.name);

export type LinkedDraft = {
  id: string;
  title: string;
  author: string;
  author_short: string;
  status: string;
  finish_at?: string;
};

/**
 * Both built from the generated governance schemas: the row is a `.pick` of
 * `ProposalRowSchema` (only the fields this drafts list reads), the envelope
 * is `ProposalsEnvelopeSchema` with its rows narrowed to that pick -- the
 * ts-rs image of catalyrst-governance's `GET /proposals`. The wire always
 * carries `user`, `status`, `finish_at`, `limit` and `offset`.
 */
const ApiProposalSchema = ProposalRowSchema.pick({
  id: true,
  title: true,
  user: true,
  status: true,
  finish_at: true,
});
const ApiEnvelopeSchema = ProposalsEnvelopeSchema.extend({
  data: z.array(ApiProposalSchema),
});

function shortAddr(addr: string): string {
  return addr.length > 12 ? `${addr.slice(0, 6)}\u{2026}${addr.slice(-4)}` : addr;
}

function toLinkedDraft(p: z.infer<typeof ApiProposalSchema>): LinkedDraft {
  const author = p.user ?? "";
  return {
    id: p.id,
    title: p.title,
    author,
    author_short: shortAddr(author),
    status: p.status ?? "passed",
    finish_at: p.finish_at ?? undefined,
  };
}

export type GovernanceProposalCopy = {
  title: string;
  description: string[];
  vpNotice: string;
  vpThreshold: number;
};

const GOVERNANCE_COPY: GovernanceProposalCopy = {
  title: "Governance proposal",
  description: [
    "The purpose of the Governance Proposal is to formalize the passed version of a Draft into a binding governance outcome. Only established or recognized community members can submit Governance Proposals, which are only passed if they reach the needed acceptance criteria for their category. In the interim period before new voting categories have been established (and for proposals that do not have a pre-set category) a Governance Proposal must receive a simple majority (51%) of participating voting power and at least 6M VP to pass as a binding decision.",
    "Processes and thresholds for established categories will not be changed as part of this proposal. Meaning, the process for grants, POIs, etc\u{2026} will remain unchanged. Additional categories for specific types of issues, e.g. \"fee structures,\" will be proposed, and relevant processes and thresholds developed.",
    "This action requires at least 2500 VP.",
  ],
  vpNotice: "This action requires at least 2500 VP.",
  vpThreshold: GOVERNANCE_SCHEMA.vpThreshold,
};

export function getGovernanceProposalCopy(): GovernanceProposalCopy {
  return GOVERNANCE_COPY;
}

export type LoadDraftsOptions = {
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
  base?: string;
  limit?: number;
};

export async function loadLinkedDrafts(
  opts: LoadDraftsOptions = {},
): Promise<{ drafts: LinkedDraft[]; live: boolean; reason: string | null }> {
  const base = governanceApiBase(opts.base);
  const limit = opts.limit ?? 6;
  const fetchWindow = Math.max(limit * 6, 40);

  let raw: unknown;
  try {
    raw = await getJSON<unknown>("/proposals", {
      base,
      query: { type: "draft", limit: fetchWindow },
      signal: opts.signal,
      fetchImpl: opts.fetchImpl,
    });
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    return { drafts: [], live: false, reason: `/proposals unreachable: ${detail}` };
  }

  const parsed = ApiEnvelopeSchema.safeParse(raw);
  if (!parsed.success) {
    return {
      drafts: [],
      live: false,
      reason: "/proposals returned an unrecognised payload",
    };
  }
  if (parsed.data.data.length === 0) {
    return { drafts: [], live: false, reason: null };
  }

  const drafts = parsed.data.data
    .filter((p) => (p.status ?? "") === "passed")
    .sort((a, b) => (b.finish_at ?? "").localeCompare(a.finish_at ?? ""))
    .slice(0, limit)
    .map(toLinkedDraft);

  if (drafts.length === 0) return { drafts: [], live: false, reason: null };
  return { drafts, live: true, reason: null };
}

export type { FieldErrors };

export function validateDetails(values: {
  linkedDraftId: string;
  title: string;
  bodies: Record<string, string>;
}): FieldErrors {
  const errors: FieldErrors = {};
  if (!values.linkedDraftId.trim()) {
    errors.linkedDraftId = "Select the passed Draft this proposal formalizes.";
  }
  const titleLen = values.title.trim().length;
  if (titleLen < GOVERNANCE_SCHEMA.title.min) {
    errors.title = `Title must be at least ${GOVERNANCE_SCHEMA.title.min} characters.`;
  } else if (titleLen > GOVERNANCE_SCHEMA.title.max) {
    errors.title = `Title must be at most ${GOVERNANCE_SCHEMA.title.max} characters.`;
  }
  for (const b of GOVERNANCE_SCHEMA.bodies) {
    const len = (values.bodies[b.name] ?? "").trim().length;
    if (len < b.min) errors[b.name] = `${b.label} is too short.`;
    else if (len > b.max) errors[b.name] = `${b.label} is too long.`;
  }
  return errors;
}

export function validateCoAuthors(coAuthors: string[]): FieldErrors {
  return sharedValidateCoAuthors(coAuthors, GOVERNANCE_SCHEMA.coAuthorsMax);
}

export type GovernanceProposalDraft = {
  linkedDraftId: string;
  title: string;
  bodies: Record<string, string>;
  coAuthors: string[];
};

export type CreatedProposal = {
  id: string;
  type: "governance";
};

export type CreateProposalFn = (args: {
  draft: GovernanceProposalDraft;
  signal?: AbortSignal;
}) => Promise<CreatedProposal>;

export type GovernanceBodyName = (typeof GOVERNANCE_SCHEMA.bodies)[number]["name"];

export type NewProposalGovernance = {
  type: "governance";
  linked_proposal_id: string;
  title: string;
  coAuthors: string[];
} & Record<GovernanceBodyName, string>;

export function buildProposalPayload(
  draft: GovernanceProposalDraft,
): NewProposalGovernance {
  const bodies = {} as Record<GovernanceBodyName, string>;
  for (const body of GOVERNANCE_SCHEMA.bodies) {
    bodies[body.name] = (draft.bodies[body.name] ?? "").trim();
  }
  return {
    type: "governance",
    linked_proposal_id: draft.linkedDraftId.trim(),
    title: draft.title.trim(),
    coAuthors: draft.coAuthors.map((a) => a.trim()).filter(Boolean),
    ...bodies,
  };
}

const SUBMIT_UNAVAILABLE =
  "governance proposal submission unavailable: DAO governance signer not configured";

export const failClosedCreateProposal: CreateProposalFn = async () => {
  throw new Error(SUBMIT_UNAVAILABLE);
};

export function buildCreateProposal(
  identity: AuthIdentity | null,
): CreateProposalFn {
  return async ({ draft, signal }) => {
    const created = await submitProposal({
      identity,
      kind: "governance",
      body: buildProposalPayload(draft),
      unavailable: SUBMIT_UNAVAILABLE,
      signal,
    });
    return { id: created.id, type: "governance" };
  };
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
export type _DriftProposalsEnvelope = Assert<
  AssignableTo<
    RsListEnvelope,
    Omit<z.input<typeof ApiEnvelopeSchema>, "data"> & {
      data: RsListEnvelope["data"];
    }
  >
>;
