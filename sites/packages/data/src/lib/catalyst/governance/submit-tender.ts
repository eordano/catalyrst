import { z } from "zod";

import config from "./submit-tender.data.json";
import { submitProposal } from "./submit-client";
import type { AuthIdentity } from "../../auth/types";
import type { ListEnvelope as RsListEnvelope } from "@ui/generated/catalyst/governance/ListEnvelope";
import { governanceApiBase } from "./api-base";
import { ETH_ADDRESS_RE } from "../format/address";
import {
  ProposalRowSchema as RsProposalRowSchema,
  ProposalsEnvelopeSchema,
} from "../generated-schemas/governance";

export { governanceApiBase };

const FieldSchema = z.object({
  label: z.string(),
  sublabel: z.string().nullish(),
  placeholder: z.string().nullish(),
  markdown: z.boolean().optional(),
  optional: z.boolean().optional(),
  required: z.boolean().optional(),
  min_length: z.number().nullish(),
  max_length: z.number().nullish(),
  max: z.number().nullish(),
  address_length: z.number().nullish(),
  help: z.string().nullish(),
});

const PitchSchema = z.object({
  id: z.string(),
  title: z.string(),
  user: z.string(),
  status: z.string(),
  start_at: z.string().nullish(),
  finish_at: z.string().nullish(),
});

const SubmitTenderSchema = z.object({
  submission_threshold_tender: z.number(),
  linked_proposal_param: z.string(),
  page: z.object({
    title: z.string(),
    description: z.string(),
    linked_proposal_label: z.string(),
    submit_label: z.string(),
  }),
  fields: z.object({
    project_name: FieldSchema,
    summary: FieldSchema,
    problem_statement: FieldSchema,
    technical_specification: FieldSchema,
    use_cases: FieldSchema,
    deliverables: FieldSchema,
    target_release_quarter: FieldSchema,
    coAuthors: FieldSchema,
  }),
  errors: z.record(z.string(), z.string()),
  default_linked_proposal_id: z.string().nullish(),
  success: z.object({
    title: z.string(),
    lead: z.string(),
    helper: z.string(),
    voting_begins_label: z.string(),
    add_to_calendar: z.string(),
    note: z.string(),
  }),
  submit_error: z.string(),
});

export type SubmitTenderField = z.infer<typeof FieldSchema>;
export type Pitch = z.infer<typeof PitchSchema>;
export type SubmitTenderData = z.infer<typeof SubmitTenderSchema>;

const FALLBACK_FIELD = (over: Partial<SubmitTenderField> & { label: string }): SubmitTenderField => ({
  markdown: false,
  optional: false,
  required: false,
  ...over,
});

const FALLBACK: SubmitTenderData = {
  submission_threshold_tender: 1000,
  linked_proposal_param: "linked_proposal_id",
  page: {
    title: "Tender proposal",
    description:
      "Tender proposals are the second step in the Bidding & Tendering process. The aim of this stage is to refine the problems outlined in the Pitch proposal, providing a clearer vision of what the execution teams should propose in their Bid proposals. This action requires at least 1000 VP.",
    linked_proposal_label: "Linked Pitch Proposal",
    submit_label: "Submit proposal",
  },
  fields: {
    project_name: FALLBACK_FIELD({ label: "Project name", min_length: 1, max_length: 80, required: true }),
    summary: FALLBACK_FIELD({ label: "Summary", markdown: true, min_length: 20, max_length: 3500, required: true }),
    problem_statement: FALLBACK_FIELD({ label: "Problem statement", markdown: true, min_length: 20, max_length: 3500, required: true }),
    technical_specification: FALLBACK_FIELD({ label: "Technical specification", markdown: true, min_length: 20, max_length: 3500, required: true }),
    use_cases: FALLBACK_FIELD({ label: "Use cases", markdown: true, min_length: 20, max_length: 3500, required: true }),
    deliverables: FALLBACK_FIELD({ label: "Deliverables", markdown: true, min_length: 20, max_length: 3500, required: true }),
    target_release_quarter: FALLBACK_FIELD({ label: "Target release quarter", max_length: 7, required: true, placeholder: "Select a target release quarter" }),
    coAuthors: FALLBACK_FIELD({ label: "Co-authors", optional: true, max: 5, address_length: 42 }),
  },
  errors: {
    submission_vp_not_met: "This action requires at least 1000 VP.",
    linked_proposal_empty: "Linked Pitch Proposal field is empty",
  },
  default_linked_proposal_id: null,
  success: {
    title: "Proposal published",
    lead: "Your proposal is now published. When the Tender submission period ends, it will be open for voting.",
    helper: "Set a reminder in your Calendar for when the voting period begins.",
    voting_begins_label: "Voting begins:",
    add_to_calendar: "Add to calendar",
    note: "This Tender was created in a simulation \u{2014} no on-chain transaction or Snapshot proposal was submitted.",
  },
  submit_error: "There was an error while trying to create the proposal, please try again later.",
};

function parse(): SubmitTenderData {
  const parsed = SubmitTenderSchema.safeParse(config);
  return parsed.success ? parsed.data : FALLBACK;
}

export function getSubmitTenderData(): SubmitTenderData {
  return parse();
}

export function getTenderVpThreshold(): number {
  return parse().submission_threshold_tender;
}

const ProposalRowSchema = RsProposalRowSchema.pick({
  id: true,
  title: true,
  type: true,
  status: true,
  user: true,
  start_at: true,
  finish_at: true,
});

/**
 * The generated `ProposalsEnvelopeSchema` (catalyrst-governance's ts-rs image
 * of `GET /proposals`) with its rows narrowed to the pick above. The wire
 * always carries `limit` and `offset`.
 */
const ProposalsResponseSchema = ProposalsEnvelopeSchema.extend({
  data: z.array(ProposalRowSchema),
});

type ProposalRow = z.infer<typeof ProposalRowSchema>;

function toPitch(row: ProposalRow): Pitch {
  return {
    id: row.id,
    title: row.title,
    user: row.user,
    status: row.status,
    start_at: row.start_at ?? null,
    finish_at: row.finish_at ?? null,
  };
}

export type LoadPitchesOptions = {
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
  limit?: number;
};

export type PitchList = {
  source: "live" | "error";
  pitches: Pitch[];
};

export async function loadPitches(opts: LoadPitchesOptions = {}): Promise<PitchList> {
  const base = governanceApiBase(opts.base);
  const limit = opts.limit ?? 200;
  const url = `${base}/proposals?limit=${limit}`;
  const doFetch = opts.fetchImpl ?? fetch;

  try {
    const res = await doFetch(url, {
      headers: { accept: "application/json" },
      signal: opts.signal,
    });
    if (!res.ok) return { source: "error", pitches: [] };
    const raw = (await res.json()) as unknown;
    const parsed = ProposalsResponseSchema.safeParse(raw);
    if (!parsed.success) return { source: "error", pitches: [] };
    const pitches = parsed.data.data
      .filter((p) => p.type === "pitch")
      .map(toPitch);
    return { source: "live", pitches };
  } catch {
    return { source: "error", pitches: [] };
  }
}

export function resolveLinkedPitch(
  pitches: Pitch[],
  id: string | null | undefined,
): Pitch | null {
  const wanted = id?.trim();
  if (wanted) {
    const hit = pitches.find((p) => p.id === wanted);
    if (hit) return hit;
  }
  const fallbackId = getSubmitTenderData().default_linked_proposal_id;
  if (fallbackId) {
    const hit = pitches.find((p) => p.id === fallbackId);
    if (hit) return hit;
  }
  return pitches[0] ?? null;
}

export function isSubmissionVpNotMet(votingPower: number, threshold = getTenderVpThreshold()): boolean {
  return votingPower < threshold;
}

export function isEthAddress(addr: string): boolean {
  return ETH_ADDRESS_RE.test(addr.trim());
}

export type TenderForm = {
  linked_proposal_id: string;
  project_name: string;
  summary: string;
  problem_statement: string;
  technical_specification: string;
  use_cases: string;
  deliverables: string;
  target_release_quarter: string;
  coAuthors: string[];
};

export function emptyTenderForm(linkedProposalId = ""): TenderForm {
  return {
    linked_proposal_id: linkedProposalId,
    project_name: "",
    summary: "",
    problem_statement: "",
    technical_specification: "",
    use_cases: "",
    deliverables: "",
    target_release_quarter: "",
    coAuthors: [],
  };
}

export function validateTenderDetails(form: TenderForm): { field: string; message: string } | null {
  const f = parse().fields;
  const checks: Array<[keyof TenderForm, SubmitTenderField, string]> = [
    ["project_name", f.project_name, "Project name"],
    ["summary", f.summary, "Summary"],
    ["problem_statement", f.problem_statement, "Problem statement"],
    ["technical_specification", f.technical_specification, "Technical specification"],
    ["use_cases", f.use_cases, "Use cases"],
    ["deliverables", f.deliverables, "Deliverables"],
  ];
  for (const [key, spec, label] of checks) {
    const v = String(form[key] ?? "").trim();
    if (v.length === 0) return { field: key, message: `${label} is empty` };
    if (spec.min_length != null && v.length < spec.min_length) {
      return { field: key, message: `${label} is too short` };
    }
    if (spec.max_length != null && v.length > spec.max_length) {
      return { field: key, message: `${label} is too large` };
    }
  }
  if (!form.target_release_quarter.trim()) {
    return { field: "target_release_quarter", message: "Target release quarter is empty" };
  }
  if (!form.linked_proposal_id.trim()) {
    return { field: "linked_proposal_id", message: "Linked Pitch Proposal field is empty" };
  }
  return null;
}

export function getQuarters(now: Date = new Date()): string[] {
  const currentQuarter = Math.floor(now.getUTCMonth() / 3) + 1;
  const currentYear = now.getUTCFullYear();
  const quarters: string[] = [];
  for (let i = 0; i < 5; i++) {
    const quarter = currentQuarter + i;
    const year = currentYear + Math.floor(quarter / 5);
    quarters.push(`${year} Q${quarter % 4 === 0 ? 4 : quarter % 4}`);
  }
  return quarters;
}

export type CreatedTender = {
  id: string;
  type: "tender";
  linked_proposal_id: string;
  pending: boolean;
};

export type CreateTenderFn = (args: {
  form: TenderForm;
  signal?: AbortSignal;
}) => Promise<CreatedTender>;

export type NewProposalTender = {
  type: "tender";
  linked_proposal_id: string;
  project_name: string;
  summary: string;
  problem_statement: string;
  technical_specification: string;
  use_cases: string;
  deliverables: string;
  target_release_quarter: string;
  coAuthors: string[];
};

export function buildTenderPayload(form: TenderForm): NewProposalTender {
  return {
    type: "tender",
    linked_proposal_id: form.linked_proposal_id.trim(),
    project_name: form.project_name.trim(),
    summary: form.summary.trim(),
    problem_statement: form.problem_statement.trim(),
    technical_specification: form.technical_specification.trim(),
    use_cases: form.use_cases.trim(),
    deliverables: form.deliverables.trim(),
    target_release_quarter: form.target_release_quarter.trim(),
    coAuthors: form.coAuthors.map((a) => a.trim()).filter(Boolean),
  };
}

const SUBMIT_UNAVAILABLE =
  "tender submission unavailable: DAO governance signer not configured";

export const failClosedCreateTender: CreateTenderFn = async () => {
  throw new Error(SUBMIT_UNAVAILABLE);
};

export function buildCreateTender(identity: AuthIdentity | null): CreateTenderFn {
  return async ({ form, signal }) => {
    const payload = buildTenderPayload(form);
    const created = await submitProposal({
      identity,
      kind: "tender",
      body: payload,
      unavailable: SUBMIT_UNAVAILABLE,
      signal,
    });
    return {
      id: created.id,
      type: "tender",
      linked_proposal_id: payload.linked_proposal_id,
      pending: created.pending ?? true,
    };
  };
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
export type _DriftProposalsEnvelope = Assert<
  AssignableTo<
    RsListEnvelope,
    Omit<z.input<typeof ProposalsResponseSchema>, "data"> & {
      data: RsListEnvelope["data"];
    }
  >
>;
