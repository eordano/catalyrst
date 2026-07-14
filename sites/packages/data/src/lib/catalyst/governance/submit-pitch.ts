import { z } from "zod";

import staticConfig from "./submit-pitch.data.json";
import { validateCoAuthors as sharedValidateCoAuthors, type FieldErrors } from "./co-authors";

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

export type { FieldErrors };

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
  return sharedValidateCoAuthors(coAuthors, PITCH_SCHEMA.coAuthors.max);
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
