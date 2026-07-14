import { z } from "zod";

import fixture from "../../../fixtures/governance-submit-council-veto.json";
import { submitProposal } from "./submit-client";
import type { AuthIdentity } from "../../auth/types";
import { validateCoAuthors as sharedValidateCoAuthors, type FieldErrors } from "./co-authors";

const FieldErrorsSchema = z
  .object({
    empty: z.string().nullish(),
    invalid: z.string().nullish(),
    too_short: z.string().nullish(),
    too_large: z.string().nullish(),
  })
  .partial();

const TextFieldSchema = z.object({
  name: z.string(),
  label: z.string(),
  detail: z.string(),
  placeholder: z.string().nullish(),
  format: z.string().nullish(),
  markdown: z.boolean().optional(),
  required: z.boolean().optional(),
  optional: z.boolean().optional(),
  min_length: z.number().nullish(),
  max_length: z.number().nullish(),
  errors: FieldErrorsSchema,
});

const CoAuthorsFieldSchema = z.object({
  name: z.string(),
  label: z.string(),
  detail: z.string(),
  optional: z.boolean(),
  max: z.number(),
  address_length: z.number(),
  placeholder: z.string().nullish(),
});

const SampleDecisionSchema = z.object({
  url: z.string(),
  snapshot_id: z.string(),
  title: z.string(),
  closed_days_ago: z.number(),
});

const CouncilVetoSchema = z.object({
  proposal_type: z.string(),
  submit_label: z.string(),
  optional_tooltip: z.string(),
  character_counter: z.string(),
  error_label: z.string(),
  submission_threshold: z.number(),
  council_snapshot_space: z.string(),
  decision_max_age_days: z.number(),
  copy: z.object({ title: z.string(), description: z.string() }),
  fields: z.object({
    decision_snapshot_id: TextFieldSchema,
    reasons: TextFieldSchema,
    suggestions: TextFieldSchema,
    coAuthors: CoAuthorsFieldSchema,
  }),
  vp_not_met_template: z.string(),
  submit_error: z.string(),
  account: z.object({
    address: z.string(),
    label: z.string(),
    votingPower: z.number(),
  }),
  sample_decision: SampleDecisionSchema,
  success: z.object({
    title: z.string(),
    lead: z.string(),
    note: z.string(),
  }),
});

export type CouncilVetoData = z.infer<typeof CouncilVetoSchema>;

const FALLBACK: CouncilVetoData = {
  proposal_type: "council_decision_veto",
  submit_label: "Submit proposal",
  optional_tooltip: "(optional)",
  character_counter: "({current} out of {limit} characters)",
  error_label:
    "There was an error while trying to create the proposal, please try again later.",
  submission_threshold: 2500,
  council_snapshot_space: "https://snapshot.org/#/dao-council.dcl.eth",
  decision_max_age_days: 14,
  copy: {
    title: "Council Decision Veto",
    description:
      "Allows the community to challenge and veto recent decisions made by the DAO Council through a governance vote.",
  },
  fields: {
    decision_snapshot_id: {
      name: "decision_snapshot_id",
      label: "DAO Council Decision (URL)",
      detail:
        "This field requires the URL of the DAO Council decision from the Council Snapshot Space. It validates two conditions: 1. The URL must belong to the Council Snapshot Space. 2. The proposal must have been closed no more than 14 days before this veto proposal is created.",
      placeholder:
        "URL of the Council decision you want to veto (must be from the Council Snapshot Space).",
      format: "url",
      markdown: false,
      required: true,
      optional: false,
      errors: {
        empty: "Council Decision URL is empty",
        invalid:
          "Couldn't read a Council decision from that URL. Use a link from the Council Snapshot Space.",
      },
    },
    reasons: {
      name: "reasons",
      label: "Reasons to Veto",
      detail:
        "Explain why you believe this decision should be vetoed. Consider including potential issues, inconsistencies, or negative impacts.",
      markdown: true,
      required: true,
      optional: false,
      min_length: 20,
      max_length: 3500,
      errors: {
        empty: "Please provide a reason for vetoing this decision",
        too_short: "Reasons field is too short",
        too_large: "Reasons field is too large",
      },
    },
    suggestions: {
      name: "suggestions",
      label: "Suggestions to the Council",
      detail:
        "Share your suggestions or alternative recommendations for the Council regarding this decision.",
      markdown: true,
      required: false,
      optional: true,
      min_length: 20,
      max_length: 3500,
      errors: {
        empty: "Please provide suggestions for the Council",
        too_short: "Suggestions field is too short",
        too_large: "Suggestions field is too large",
      },
    },
    coAuthors: {
      name: "coAuthors",
      label: "Co-authors",
      detail:
        "If you co-authored this proposal with someone else, you can add their wallet addresses to acknowledge their work. After you publish the proposal, co-authors will be asked to confirm or reject the request. Only if they confirm, they will be listed publicly on the proposal page.",
      optional: true,
      max: 5,
      address_length: 42,
      placeholder: "Enter a wallet address",
    },
  },
  vp_not_met_template: "This action requires at least {threshold} VP.",
  submit_error:
    "There was an error while trying to create the proposal, please try again later.",
  account: {
    address: "",
    label: "",
    votingPower: 0,
  },
  sample_decision: {
    url: "https://snapshot.org/#/dao-council.dcl.eth/proposal/0xsample",
    snapshot_id: "0xsample",
    title: "Council Decision \u{2014} sample",
    closed_days_ago: 3,
  },
  success: {
    title: "Proposal created",
    lead: "Your Council Decision Veto proposal is now live for the DAO to vote on.",
    note: "This proposal was created in a simulation \u{2014} no on-chain transaction or Snapshot proposal was submitted.",
  },
};

function parse(): CouncilVetoData {
  const parsed = CouncilVetoSchema.safeParse(fixture);
  return parsed.success ? parsed.data : FALLBACK;
}

export function getSubmitCouncilVetoData(): CouncilVetoData {
  return parse();
}

export const COUNCIL_VETO_SCHEMA = {
  reasons: { min: 20, max: 3500 },
  suggestions: { min: 20, max: 3500 },
  coAuthors: { max: 5, addressLength: 42 },
} as const;

export type DecisionRef = {
  url: string;
  snapshotId: string;
  space: string;
  valid: boolean;
};

export function councilSpaceId(raw?: string): string {
  const source = (raw ?? parse().council_snapshot_space).trim();
  const afterHash = source.split("#/").pop() ?? "";
  const segment = afterHash.split("/").filter(Boolean)[0] ?? "";
  return segment.toLowerCase();
}

const INVALID_DECISION: Omit<DecisionRef, "url"> = {
  snapshotId: "",
  space: "",
  valid: false,
};

export function parseDecisionUrl(raw: string, councilSpace?: string): DecisionRef {
  const url = raw.trim();
  if (!url) return { url, ...INVALID_DECISION };
  try {
    const target = new URL(url);
    if (!/(^|\.)snapshot\.org$/i.test(target.hostname)) {
      return { url, ...INVALID_DECISION };
    }
    const segments = target.hash.replace(/^#\/?/, "").split("/").filter(Boolean);
    const [space, section, id] = segments;
    if (section !== "proposal" || !id) {
      return { url, ...INVALID_DECISION, space: space ?? "" };
    }
    const expected = councilSpaceId(councilSpace);
    return {
      url,
      snapshotId: id,
      space: space ?? "",
      valid: Boolean(expected) && (space ?? "").toLowerCase() === expected,
    };
  } catch {
    return { url, ...INVALID_DECISION };
  }
}

export type { FieldErrors };

export function validateDecisionUrl(url: string): FieldErrors {
  const f = parse().fields.decision_snapshot_id;
  const errors: FieldErrors = {};
  if (url.trim() === "") {
    errors.decision_snapshot_id = f.errors.empty ?? "Council Decision URL is empty";
    return errors;
  }
  if (!parseDecisionUrl(url).valid) {
    errors.decision_snapshot_id =
      f.errors.invalid ?? "Couldn't read a Council decision from that URL.";
  }
  return errors;
}

export function validateReasons(reasons: string): FieldErrors {
  const f = parse().fields.reasons;
  const errors: FieldErrors = {};
  const len = reasons.trim().length;
  if (len === 0) errors.reasons = f.errors.empty ?? "Please provide a reason for vetoing this decision";
  else if (len < COUNCIL_VETO_SCHEMA.reasons.min)
    errors.reasons = f.errors.too_short ?? "Reasons field is too short";
  else if (len > COUNCIL_VETO_SCHEMA.reasons.max)
    errors.reasons = f.errors.too_large ?? "Reasons field is too large";
  return errors;
}

export function validateSuggestions(suggestions: string): FieldErrors {
  const f = parse().fields.suggestions;
  const errors: FieldErrors = {};
  const len = suggestions.trim().length;
  if (len === 0) return errors;
  if (len < COUNCIL_VETO_SCHEMA.suggestions.min)
    errors.suggestions = f.errors.too_short ?? "Suggestions field is too short";
  else if (len > COUNCIL_VETO_SCHEMA.suggestions.max)
    errors.suggestions = f.errors.too_large ?? "Suggestions field is too large";
  return errors;
}

export function validateCoAuthors(coAuthors: string[]): FieldErrors {
  return sharedValidateCoAuthors(coAuthors, COUNCIL_VETO_SCHEMA.coAuthors.max);
}

export type NewProposalCouncilVeto = {
  type: "council_decision_veto";
  decision_snapshot_id: string;
  reasons: string;
  suggestions?: string;
  coAuthors: string[];
};

export type CreatedProposal = {
  id: string;
  type: "council_decision_veto";
  decision_snapshot_id: string;
};

export function buildProposalPayload(input: {
  decisionUrl: string;
  reasons: string;
  suggestions: string;
  coAuthors: string[];
}): NewProposalCouncilVeto {
  const snapshotId = parseDecisionUrl(input.decisionUrl).snapshotId;
  const suggestions = input.suggestions.trim();
  return {
    type: "council_decision_veto",
    decision_snapshot_id: snapshotId,
    reasons: input.reasons.trim(),
    suggestions: suggestions || undefined,
    coAuthors: input.coAuthors.map((a) => a.trim()).filter(Boolean),
  };
}

export type CreateProposalFn = (args: {
  payload: NewProposalCouncilVeto;
  signal?: AbortSignal;
}) => Promise<CreatedProposal>;

const SUBMIT_UNAVAILABLE =
  "council veto submission unavailable: DAO governance signer not configured";

export const failClosedCreateProposal: CreateProposalFn = async () => {
  throw new Error(SUBMIT_UNAVAILABLE);
};

export function buildCreateProposal(
  identity: AuthIdentity | null,
): CreateProposalFn {
  return async ({ payload, signal }) => {
    const created = await submitProposal({
      identity,
      kind: "council-decision-veto",
      body: payload,
      unavailable: SUBMIT_UNAVAILABLE,
      signal,
    });
    return {
      id: created.id,
      type: "council_decision_veto",
      decision_snapshot_id: payload.decision_snapshot_id,
    };
  };
}
