import { z } from "zod";

import staticConfig from "./submit-pitch.data.json";

export const PITCH_SCHEMA = {
  initiative_name: { min: 1, max: 80 },
  problem_statement: { min: 20, max: 3500 },
  proposed_solution: { min: 20, max: 3500 },
  target_audience: { min: 20, max: 3500 },
  relevance: { min: 20, max: 3500 },
  coAuthors: { max: 5, addressLength: 42 },
} as const;

export const MARKDOWN_FIELDS = [
  "problem_statement",
  "proposed_solution",
  "target_audience",
  "relevance",
] as const;
export type MarkdownField = (typeof MARKDOWN_FIELDS)[number];

const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

export const PitchProposalSchema = z.object({
  initiative_name: z
    .string()
    .min(PITCH_SCHEMA.initiative_name.min, "An initiative name is required.")
    .max(PITCH_SCHEMA.initiative_name.max, "This initiative name is too long."),
  problem_statement: z
    .string()
    .min(PITCH_SCHEMA.problem_statement.min, "This problem statement is too short.")
    .max(PITCH_SCHEMA.problem_statement.max, "This problem statement is too long."),
  proposed_solution: z
    .string()
    .min(PITCH_SCHEMA.proposed_solution.min, "This proposed solution is too short.")
    .max(PITCH_SCHEMA.proposed_solution.max, "This proposed solution is too long."),
  target_audience: z
    .string()
    .min(PITCH_SCHEMA.target_audience.min, "This target audience is too short.")
    .max(PITCH_SCHEMA.target_audience.max, "This target audience is too long."),
  relevance: z
    .string()
    .min(PITCH_SCHEMA.relevance.min, "This relevance section is too short.")
    .max(PITCH_SCHEMA.relevance.max, "This relevance section is too long."),
  coAuthors: z
    .array(z.string().regex(ADDRESS_RE, "Co-author must be a wallet address."))
    .max(PITCH_SCHEMA.coAuthors.max, "You can add at most 5 co-authors.")
    .optional(),
});

export type FieldErrors = Record<string, string>;

const FIELD_LABELS: Record<string, string> = {
  initiative_name: "initiative name",
  problem_statement: "problem statement",
  proposed_solution: "proposed solution",
  target_audience: "target audience",
  relevance: "relevance section",
};

function validateLength(
  field: keyof typeof PITCH_SCHEMA,
  value: string,
): string {
  const { min, max } = PITCH_SCHEMA[field] as { min: number; max: number };
  const len = value.trim().length;
  const label = FIELD_LABELS[field] ?? field;
  if (len < min) {
    return min === 1
      ? "An initiative name is required."
      : `This ${label} is too short.`;
  }
  if (value.length > max) return `This ${label} is too long.`;
  return "";
}

export type PitchDetails = {
  initiative_name: string;
  problem_statement: string;
  proposed_solution: string;
  target_audience: string;
  relevance: string;
};

export function validateDetails(details: PitchDetails): FieldErrors {
  const errors: FieldErrors = {};
  const name = validateLength("initiative_name", details.initiative_name);
  if (name) errors.initiative_name = name;
  for (const field of MARKDOWN_FIELDS) {
    const err = validateLength(field, details[field]);
    if (err) errors[field] = err;
  }
  return errors;
}

export function validateCoAuthors(coAuthors: string[]): FieldErrors {
  const errors: FieldErrors = {};
  if (coAuthors.length > PITCH_SCHEMA.coAuthors.max) {
    errors.coAuthors = "You can add at most 5 co-authors.";
    return errors;
  }
  for (const addr of coAuthors) {
    if (!ADDRESS_RE.test(addr)) {
      errors.coAuthors = "Co-author must be a wallet address.";
      break;
    }
  }
  return errors;
}

const FieldCopySchema = z.object({
  label: z.string(),
  detail: z.string(),
  placeholder: z.string(),
});

const StaticConfigSchema = z.object({
  submissionThresholdVp: z.number(),
  votingPowerToPassVp: z.number(),
  copy: z.object({
    title: z.string(),
    description: z.string(),
    vpNotice: z.string(),
    initiativeNameLabel: z.string(),
    initiativeNamePostLabel: z.string(),
    fields: z.object({
      problem_statement: FieldCopySchema,
      proposed_solution: FieldCopySchema,
      target_audience: FieldCopySchema,
      relevance: FieldCopySchema,
    }),
    coAuthorLabel: z.string(),
    coAuthorDescription: z.string(),
  }),
  sample: z.object({
    initiative_name: z.string(),
    problem_statement: z.string(),
    proposed_solution: z.string(),
    target_audience: z.string(),
    relevance: z.string(),
  }),
});

export type PitchConfig = z.infer<typeof StaticConfigSchema>;

const STATIC: PitchConfig = StaticConfigSchema.parse(staticConfig);

export type PitchSubmitContext = {
  copy: PitchConfig["copy"];
  sample: PitchConfig["sample"];
  schema: typeof PITCH_SCHEMA;
};

export function getPitchSubmitContext(): PitchSubmitContext {
  return {
    copy: STATIC.copy,
    sample: STATIC.sample,
    schema: PITCH_SCHEMA,
  };
}
