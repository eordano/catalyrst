import { z } from "zod";

import staticConfig from "./submit-poi.data.json";

export const POI_REQUESTS = ["add", "remove"] as const;
export type PoiRequest = (typeof POI_REQUESTS)[number];

export const POI_TYPE = {
  add: "add_poi",
  remove: "remove_poi",
} as const;
export type PoiType = (typeof POI_TYPE)[PoiRequest];

export function toPoiRequest(value: string | null | undefined): PoiRequest | null {
  if (value === "add" || value === "remove") return value;
  return null;
}

export const POI_SCHEMA = {
  x: { min: -150, max: 163 },
  y: { min: -150, max: 159 },
  description: { min: 20, max: 250 },
  coAuthors: { max: 5, addressLength: 42 },
} as const;

export const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

function coordSchema(axis: "x" | "y") {
  const { min, max } = POI_SCHEMA[axis];
  return z
    .number({ message: `Enter the ${axis.toUpperCase()} coordinate.` })
    .int("Coordinates must be whole numbers.")
    .min(min, "These coordinates are outside of the map limits.")
    .max(max, "These coordinates are outside of the map limits.");
}

export const PoiProposalSchema = z.object({
  type: z.enum([POI_TYPE.add, POI_TYPE.remove]),
  x: coordSchema("x"),
  y: coordSchema("y"),
  description: z
    .string()
    .min(POI_SCHEMA.description.min, "This description is too short.")
    .max(POI_SCHEMA.description.max, "This description is too long."),
  coAuthors: z
    .array(z.string().regex(ADDRESS_RE, "Co-author must be a wallet address."))
    .max(POI_SCHEMA.coAuthors.max, "You can add at most 5 co-authors.")
    .optional(),
});

export type FieldErrors = Record<string, string>;

export function validateCoordinates(x: string, y: string): FieldErrors {
  const errors: FieldErrors = {};
  const check = (raw: string, axis: "x" | "y") => {
    if (raw.trim() === "") {
      errors[axis] = `Enter the ${axis.toUpperCase()} coordinate.`;
      return;
    }
    const n = Number(raw);
    if (!Number.isFinite(n) || !Number.isInteger(n)) {
      errors[axis] = "Coordinates must be whole numbers.";
      return;
    }
    const { min, max } = POI_SCHEMA[axis];
    if (n < min || n > max) {
      errors[axis] = "These coordinates are outside of the map limits.";
    }
  };
  check(x, "x");
  check(y, "y");
  return errors;
}

export function validateDescription(description: string): FieldErrors {
  const errors: FieldErrors = {};
  const len = description.trim().length;
  if (len < POI_SCHEMA.description.min) errors.description = "This description is too short.";
  else if (len > POI_SCHEMA.description.max) errors.description = "This description is too long.";
  return errors;
}

export function validateCoAuthors(coAuthors: string[]): FieldErrors {
  const errors: FieldErrors = {};
  if (coAuthors.length > POI_SCHEMA.coAuthors.max) {
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

const CopyVariantSchema = z.object({
  title: z.string(),
  coordinatesLabel: z.string(),
  descriptionDetail: z.string(),
  descriptionPlaceholder: z.string(),
});

const SampleSchema = z.object({ x: z.number(), y: z.number() });

const StaticConfigSchema = z.object({
  votingPowerToPass: z.number(),
  copy: z.object({
    intro: z.string(),
    add: CopyVariantSchema,
    remove: CopyVariantSchema,
  }),
  samples: z.object({ add: SampleSchema, remove: SampleSchema }),
});

export type Fixture = z.infer<typeof StaticConfigSchema>;

const FIXTURE: Fixture = StaticConfigSchema.parse(staticConfig);

export type PoiAccount = {
  address: string;
  label: string;
  votingPower: number | null;
};

export type PoiCopyContext = {
  request: PoiRequest;
  poiType: PoiType;
  title: string;
  intro: string;
  coordinatesLabel: string;
  descriptionDetail: string;
  descriptionPlaceholder: string;
  sample: { x: number; y: number };
  votingPowerToPass: number;
  schema: typeof POI_SCHEMA;
};

export type PoiSubmitContext = PoiCopyContext & { account: PoiAccount };

export function getPoiSubmitContext(request: PoiRequest): PoiCopyContext {
  const copy = FIXTURE.copy[request];
  return {
    request,
    poiType: POI_TYPE[request],
    title: copy.title,
    intro: FIXTURE.copy.intro,
    coordinatesLabel: copy.coordinatesLabel,
    descriptionDetail: copy.descriptionDetail,
    descriptionPlaceholder: copy.descriptionPlaceholder,
    sample: FIXTURE.samples[request],
    votingPowerToPass: FIXTURE.votingPowerToPass,
    schema: POI_SCHEMA,
  };
}
