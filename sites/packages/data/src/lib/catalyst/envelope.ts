import { z } from "zod";

import { ApiOkSchema } from "./generated-schemas/events";
import { DataTotalSchema } from "./generated-schemas/market";
import { ApiDataSchema, ApiDataTotalSchema } from "./generated-schemas/places";

export { ApiDataSchema, ApiDataTotalSchema };

export const dataTotalOf = DataTotalSchema;

export const apiOkOf = ApiOkSchema;

export const dataOf = <T extends z.ZodTypeAny>(inner: T) =>
  z.object({ data: inner });

export const okDataTotalOf = <T extends z.ZodTypeAny>(inner: T) =>
  ApiDataSchema(inner).extend({ total: z.number().optional() });
