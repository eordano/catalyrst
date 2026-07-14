import { z } from "zod";

import { getJSON } from "../client";
import type { Query } from "../client";
import { submitProposal } from "./submit-client";
import type { AuthIdentity } from "../../auth/types";
import type { ListEnvelope as RsListEnvelope } from "@ui/generated/catalyst/governance/ListEnvelope";
import { governanceApiBase } from "./api-base";

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

const ParentSchema = z.object({
  id: z.string(),
  type: z.string(),
  kind: z.string(),
  title: z.string(),
  status: z.string(),
  requiredToPass: z.number().nullish(),
  votingPowerLabel: z.string(),
  finishAt: z.string(),
  finishedLabel: z.string(),
  linkedProposalId: z.string().nullish(),
  url: z.string(),
});

export type BidParent = z.infer<typeof ParentSchema>;
export type ConsentItem = { key: string; text: string };

export type BidSampleDraft = {
  funding: number;
  projectDuration: number;
  deliveryDate: string;
  beneficiary: string;
  email: string;
  teamName: string;
  deliverables: string;
  roadmap: string;
  milestones: { title: string; tasks: string }[];
  members: { name: string; role: string }[];
  budgetBreakdown: { concept: string; amount: string }[];
  coAuthors: string[];
};

export type SubmitBidData = {
  request_param: string;
  account: { address: string; label: string; votingPower: number };
  parents: { pitch: BidParent; tender: BidParent };
  tenders: BidParent[];
  fallback: boolean;
  copy: { title: string; description: string; submit_label: string };
  funding: Record<string, unknown>;
  general: Record<string, unknown>;
  team: Record<string, unknown>;
  dueDiligence: Record<string, unknown>;
  finalConsent: { label: string; items: ConsentItem[] };
  sampleDraft: BidSampleDraft;
  submit_error: string;
  success: {
    variant: string;
    title: string;
    description: string;
    helper: string;
    votingStartsAt: string;
    note: string;
  };
};

const COPY = {
  title: "Submit Bid proposal",
  description:
    "Part of the Bidding & Tendering process, bid proposals are meant for professional teams to scope and propose a project out of their own understanding of one given issue or desire outlined by the Community based on the two preceding instances that would have passed. Only one Bid Proposal per a given Tender Proposal will be funded.",
  submit_label: "Submit proposal",
} as const;

const FUNDING_SPEC = {
  budget: { label: "Budget", unit: "USD", min: 100, max: 240000, placeholder: "100-240000", type: "integer" },
  projectDuration: { label: "Project duration", unit: "months", min: 1, max: 12, type: "integer" },
  deliveryDate: { label: "Delivery date", type: "string", required: true },
  beneficiary: {
    label: "Beneficiary address",
    sublabel:
      "The address that will receive the grant funds. This must be an Ethereum address! Entering a non-Ethereum address that cannot receive MANA may result in a permanent loss of funds.",
    format: "address",
    placeholder: "0x\u{2026}",
    required: true,
  },
  email: {
    label: "Contact Email Address",
    sublabel:
      "This email address will be used by the Grant Support teams to contact you to check the progress of the grant, set up meetings, and maintain an open communication channel.",
    postlabel:
      "Note: The address will be published in the proposal and publicly visible. If you want to keep your anonymity consider using an email address without personally identifiable information.",
    format: "email",
    placeholder: "Enter your email address",
    required: true,
  },
} as const;

const GENERAL_SPEC = {
  teamName: { label: "Team Name", minLength: 1, maxLength: 80, required: true },
  deliverables: {
    label: "Deliverables",
    sublabel:
      "Be as specific as possible. Describe the entire scope of the project and the actual work you are going to deliver to the DAO.",
    minLength: 20,
    maxLength: 1500,
    markdown: true,
    required: true,
  },
  roadmap: {
    label: "Roadmap",
    sublabel: "Describe the main phases or steps your project will follow to reach its goal.",
    placeholder:
      "Your estimated timeline and key milestones. Include your plan for reporting progress to the community.",
    minLength: 20,
    maxLength: 1500,
    markdown: true,
    required: true,
  },
  milestones: {
    label: "Milestones",
    sublabel: "Identify the important goals or checkpoints you want to achieve during your project.",
    maxItems: 10,
  },
  coAuthors: { label: "Co-authors", optional: true, max: 5, addressLength: 42 },
} as const;

const TEAM_SPEC = {
  members: {
    label: "Members",
    sublabel:
      "Please list who will be working on this project and include an explicit overview of their relevant skillset and experience. You may provide links to portfolios or profiles to help the Decentraland community get to know who the DAO will be funding and how their backgrounds will contribute to your project's success.",
    minItems: 1,
    required: true,
  },
} as const;

const DUE_DILIGENCE_SPEC = {
  budgetBreakdown: {
    label: "Budget breakdown",
    sublabel:
      "Please provide a detailed specification on how you will be using the funds requested for this Grant. Our community values transparency, so be as specific as possible.",
    minItems: 1,
    required: true,
  },
} as const;

const FINAL_CONSENT: { label: string; items: ConsentItem[] } = {
  label: "Review and check the following",
  items: [
    { key: "contentPolicy", text: "I've read and understood Decentraland's Content Policy" },
    { key: "termsOfUse", text: "I've read Decentraland's Terms of Use and agree with them" },
    { key: "codeOfEthics", text: "I've read Decentraland's Code of Ethics and hereby commit to honoring it" },
  ],
};

const SAMPLE_DRAFT: BidSampleDraft = {
  funding: 90000,
  projectDuration: 4,
  deliveryDate: "2026-10-30",
  beneficiary: "0x0000000000000000000000000000000000000000",
  email: "team@example.org",
  teamName: "Genesis Onboarding Guild",
  deliverables:
    "Ship a guided first-session experience for new wallets: a contextual HUD walkthrough, a starter quest, and analytics instrumentation across the onboarding funnel.",
  roadmap:
    "Phase 1 discovery and funnel analysis, phase 2 build and instrument the guided session, phase 3 beta and iteration based on retention data.",
  milestones: [
    { title: "2026-08-01 - Discovery & research", tasks: "User interviews, funnel analytics, scope lock" },
    { title: "2026-09-15 - Beta release", tasks: "QA, instrumentation, launch" },
  ],
  members: [
    { name: "ada.dcl", role: "Lead engineer" },
    { name: "grace.dcl", role: "Designer" },
  ],
  budgetBreakdown: [{ concept: "Engineering (3 devs)", amount: "$90,000 \u{B7} 4 months" }],
  coAuthors: [],
};

const SUCCESS = {
  variant: "bid",
  title: "Bid submitted but not published",
  description:
    "Your bid has been successfully submitted and stored. It will remain private until the voting period starts. It will only be published for voting if at least one competing bid for the same Tender exists.",
  helper: "Set a reminder in your Calendar for when the voting period begins.",
  votingStartsAt: "Jul 25, 2026 09:30",
  note: "This Bid was created in a simulation \u{2014} no on-chain transaction or Snapshot proposal was submitted.",
} as const;

const SUBMIT_ERROR =
  "There was an error while trying to create the proposal, please try again later.";

const GUEST_ACCOUNT = { address: "", label: "Guest", votingPower: 0 } as const;

const STATIC = {
  request_param: "linked_proposal_id",
  account: { ...GUEST_ACCOUNT },
  copy: { ...COPY },
  funding: FUNDING_SPEC as unknown as Record<string, unknown>,
  general: GENERAL_SPEC as unknown as Record<string, unknown>,
  team: TEAM_SPEC as unknown as Record<string, unknown>,
  dueDiligence: DUE_DILIGENCE_SPEC as unknown as Record<string, unknown>,
  finalConsent: FINAL_CONSENT,
  sampleDraft: SAMPLE_DRAFT,
  submit_error: SUBMIT_ERROR,
  success: { ...SUCCESS },
};

function stubParent(type: "pitch" | "tender", kind: string, label: string): BidParent {
  return {
    id: "",
    type,
    kind,
    title: `${label} (live data unavailable)`,
    status: "",
    requiredToPass: null,
    votingPowerLabel: "\u{2014}",
    finishAt: "",
    finishedLabel: "passed",
    linkedProposalId: null,
    url: "https://governance.decentraland.org/",
  };
}

const STUB_PARENTS = {
  pitch: stubParent("pitch", "Parent Pitch", "Parent Pitch"),
  tender: stubParent("tender", "Parent Tender", "Parent Tender"),
};

const ProposalRowSchema = z.object({
  id: z.string(),
  title: z.string().nullish(),
  type: z.string().nullish(),
  status: z.string().nullish(),
  finish_at: z.string().nullish(),
  required_to_pass: z.number().nullish(),
  configuration: z
    .object({ linked_proposal_id: z.string().nullish() })
    .passthrough()
    .nullish(),
});

const EnvelopeSchema = z.object({ data: z.array(z.unknown()).nullish() });

type ProposalRow = z.infer<typeof ProposalRowSchema>;

const VP_FMT = new Intl.NumberFormat("en-US");

function finishedLabelFor(status: string | null | undefined): string {
  const s = (status ?? "").toLowerCase();
  if (s === "passed" || s === "enacted") return "passed";
  if (s === "rejected") return "rejected";
  if (s === "finished") return "finished";
  return s || "passed";
}

function proposalUrl(id: string): string {
  return `https://governance.decentraland.org/proposal/?id=${id}`;
}

function toParent(row: ProposalRow, type: "pitch" | "tender", kind: string): BidParent {
  const req = row.required_to_pass ?? null;
  return {
    id: row.id,
    type,
    kind,
    title: row.title ?? row.id,
    status: row.status ?? "",
    requiredToPass: req,
    votingPowerLabel: req != null ? VP_FMT.format(req) : "\u{2014}",
    finishAt: row.finish_at ?? "",
    finishedLabel: finishedLabelFor(row.status),
    linkedProposalId: row.configuration?.linked_proposal_id ?? null,
    url: proposalUrl(row.id),
  };
}

export type GetSubmitBidOptions = {
  linkedProposalId?: string | null;
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
};

async function fetchProposals(
  query: Query,
  base: string,
  opts: { signal?: AbortSignal; fetchImpl?: typeof fetch },
): Promise<ProposalRow[] | null> {
  let raw: unknown;
  try {
    raw = await getJSON<unknown>("/proposals", {
      base,
      query,
      signal: opts.signal,
      fetchImpl: opts.fetchImpl,
    });
  } catch {
    return null;
  }
  const env = EnvelopeSchema.safeParse(raw);
  if (!env.success) return null;
  const out: ProposalRow[] = [];
  for (const row of env.data.data ?? []) {
    const parsed = ProposalRowSchema.safeParse(row);
    if (parsed.success) out.push(parsed.data);
  }
  return out;
}

export async function getSubmitBidData(
  opts: GetSubmitBidOptions = {},
): Promise<SubmitBidData> {
  const base = governanceApiBase(opts.base);
  const fetchOpts = { signal: opts.signal, fetchImpl: opts.fetchImpl };

  const [tenderRows, pitchRows] = await Promise.all([
    fetchProposals({ type: "tender", limit: 100 }, base, fetchOpts),
    fetchProposals({ type: "pitch", limit: 200 }, base, fetchOpts),
  ]);

  if (tenderRows === null || tenderRows.length === 0) {
    return {
      ...STATIC,
      parents: { pitch: { ...STUB_PARENTS.pitch }, tender: { ...STUB_PARENTS.tender } },
      tenders: [],
      fallback: true,
    };
  }

  const wanted = opts.linkedProposalId?.trim();
  const selectedRow =
    (wanted ? tenderRows.find((r) => r.id === wanted) : undefined) ?? tenderRows[0];
  const tenderVm = toParent(selectedRow, "tender", "Parent Tender");

  const pitchId = selectedRow.configuration?.linked_proposal_id ?? null;
  let pitchRow: ProposalRow | null = pitchId
    ? pitchRows?.find((r) => r.id === pitchId) ?? null
    : null;
  if (pitchId && !pitchRow) {
    const byId = await fetchProposals({ id: pitchId, limit: 1 }, base, fetchOpts);
    pitchRow = byId?.find((r) => r.id === pitchId) ?? null;
  }
  const pitchVm = pitchRow
    ? toParent(pitchRow, "pitch", "Parent Pitch")
    : { ...STUB_PARENTS.pitch };

  return {
    ...STATIC,
    parents: { pitch: pitchVm, tender: tenderVm },
    tenders: tenderRows.map((r) => toParent(r, "tender", "Parent Tender")),
    fallback: false,
  };
}

export type FieldErrors = Record<string, string>;

export type FundingDraft = {
  funding: number | null;
  projectDuration: number;
  deliveryDate: string;
  beneficiary: string;
  email: string;
};

export type GeneralDraft = {
  teamName: string;
  deliverables: string;
  roadmap: string;
  milestones: { title: string; tasks: string }[];
  members: { name: string; role: string }[];
  budgetBreakdown: { concept: string; amount: string }[];
  coAuthors: string[];
  consent: { contentPolicy: boolean; termsOfUse: boolean; codeOfEthics: boolean };
};

export type NewProposalBid = {
  linked_proposal_id: string;
  type: "bid";
  funding: number;
  projectDuration: number;
  deliveryDate: string;
  beneficiary: string;
  email: string;
  teamName: string;
  deliverables: string;
  roadmap: string;
  milestones: { title: string; tasks: string }[];
  members: { name: string; role: string }[];
  budgetBreakdown: { concept: string; amount: string }[];
  coAuthors: string[];
};

export type NewProposalBidFunding = {
  type: "bid";
  linked_proposal_id: string;
  funding: number;
  projectDuration: number;
};

export type SubmittedBid = {
  proposalId: string;
  published: boolean;
};

export type SubmitBidFn = (args: {
  tenderId: string;
  budget: number;
  duration: number;
  signal?: AbortSignal;
}) => Promise<SubmittedBid>;

const SUBMIT_UNAVAILABLE =
  "bid submission unavailable: DAO governance signer not configured";

export const failClosedSubmitBid: SubmitBidFn = async () => {
  throw new Error(SUBMIT_UNAVAILABLE);
};

export function buildSubmitBid(identity: AuthIdentity | null): SubmitBidFn {
  return async ({ tenderId, budget, duration, signal }) => {
    const payload: NewProposalBidFunding = {
      type: "bid",
      linked_proposal_id: tenderId.trim(),
      funding: budget,
      projectDuration: duration,
    };
    const created = await submitProposal({
      identity,
      kind: "bid",
      body: payload,
      unavailable: SUBMIT_UNAVAILABLE,
      signal,
    });
    return { proposalId: created.id, published: created.published ?? false };
  };
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
export type _DriftProposalsEnvelope = Assert<
  AssignableTo<RsListEnvelope, z.input<typeof EnvelopeSchema>>
>;
