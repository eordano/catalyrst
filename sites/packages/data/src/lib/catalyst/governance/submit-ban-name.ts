import { z } from "zod";

import fixture from "../../../fixtures/governance-submit-ban-name.json";

export const BAN_NAME_SCHEMA = {
  name: { min: 2, max: 15 },
  description: { min: 20, max: 250 },
  coAuthors: { max: 5, addressLength: 42 },
} as const;

const NAME_RE = /^[a-zA-Z0-9]{2,15}$/;

const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

export const BanNameProposalSchema = z.object({
  name: z
    .string()
    .min(BAN_NAME_SCHEMA.name.min, "Enter a name between 2 and 15 characters.")
    .max(BAN_NAME_SCHEMA.name.max, "Enter a name between 2 and 15 characters.")
    .regex(NAME_RE, "Names can only contain letters and numbers."),
  description: z
    .string()
    .min(BAN_NAME_SCHEMA.description.min, "This description is too short.")
    .max(BAN_NAME_SCHEMA.description.max, "This description is too long."),
  coAuthors: z
    .array(z.string().regex(ADDRESS_RE, "Co-author must be a wallet address."))
    .max(BAN_NAME_SCHEMA.coAuthors.max, "You can add at most 5 co-authors.")
    .optional(),
});

export type FieldErrors = Record<string, string>;

export function validateName(name: string): FieldErrors {
  const errors: FieldErrors = {};
  const trimmed = name.trim();
  if (trimmed.length < BAN_NAME_SCHEMA.name.min || trimmed.length > BAN_NAME_SCHEMA.name.max) {
    errors.name = "Enter a name between 2 and 15 characters.";
  } else if (!NAME_RE.test(trimmed)) {
    errors.name = "Names can only contain letters and numbers.";
  }
  return errors;
}

export function validateDescription(description: string): FieldErrors {
  const errors: FieldErrors = {};
  const len = description.trim().length;
  if (len < BAN_NAME_SCHEMA.description.min) errors.description = "This description is too short.";
  else if (len > BAN_NAME_SCHEMA.description.max) errors.description = "This description is too long.";
  return errors;
}

export function validateCoAuthors(coAuthors: string[]): FieldErrors {
  const errors: FieldErrors = {};
  if (coAuthors.length > BAN_NAME_SCHEMA.coAuthors.max) {
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

type Fixture = {
  _source: string;
  account: { address: string; label: string; votingPower: number };
  copy: {
    title: string;
    description: string;
    nameLabel: string;
    namePlaceholder: string;
    descriptionLabel: string;
    descriptionDetail: string;
    descriptionPlaceholder: string;
    submitLabel: string;
    noVpGate: boolean;
  };
  sample: { name: string; description: string; coAuthors: string[] };
};

const FIXTURE = fixture as unknown as Fixture;

export type BanNameSubmitContext = {
  title: string;
  intro: string;
  nameLabel: string;
  namePlaceholder: string;
  descriptionLabel: string;
  descriptionDetail: string;
  descriptionPlaceholder: string;
  submitLabel: string;
  account: { address: string; label: string; votingPower: number };
  sample: { name: string; description: string; coAuthors: string[] };
  schema: typeof BAN_NAME_SCHEMA;
};

export function getBanNameSubmitContext(): BanNameSubmitContext {
  const { copy } = FIXTURE;
  return {
    title: copy.title,
    intro: copy.description,
    nameLabel: copy.nameLabel,
    namePlaceholder: copy.namePlaceholder,
    descriptionLabel: copy.descriptionLabel,
    descriptionDetail: copy.descriptionDetail,
    descriptionPlaceholder: copy.descriptionPlaceholder,
    submitLabel: copy.submitLabel,
    account: FIXTURE.account,
    sample: FIXTURE.sample,
    schema: BAN_NAME_SCHEMA,
  };
}
