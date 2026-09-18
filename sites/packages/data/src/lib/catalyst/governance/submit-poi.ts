import { z } from "zod";

import staticConfig from "./submit-poi.data.json";
import { type FieldErrors } from "./co-authors";

const POI_REQUESTS = ["add", "remove"] as const;
export type PoiRequest = (typeof POI_REQUESTS)[number];

const POI_TYPE = {
  add: "add_poi",
  remove: "remove_poi",
} as const;
type PoiType = (typeof POI_TYPE)[PoiRequest];

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

export type { FieldErrors };

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

type Fixture = z.infer<typeof StaticConfigSchema>;

const FIXTURE: Fixture = StaticConfigSchema.parse(staticConfig);

export type PoiAccount = {
  address: string;
  label: string;
  votingPower: number | null;
};

type PoiCopyContext = {
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
