import { z } from "zod";

import { CatalystError, getJSON } from "../client";
import type { GetOptions } from "../client";
import { BidSchema, BidsEnvelopeSchema } from "../generated-schemas/market";
import { warnInvalid } from "../warn";

export { BidSchema, BidsEnvelopeSchema };

/** The wire bid row, exactly as `/market/v1/bids` reports it. */
export type Bid = z.infer<typeof BidSchema>;

export async function fetchOpenBids(
  contractAddress: string,
  itemId: string,
  opts: GetOptions = {},
): Promise<Bid[]> {
  const path = "/market/v1/bids";
  const raw = await getJSON<unknown>(path, {
    ...opts,
    query: { contractAddress, itemId, status: "open", first: 20 },
  });
  const env = BidsEnvelopeSchema.safeParse(raw);
  if (!env.success) {
    warnInvalid("BidsEnvelope", env.error.issues);
    throw new CatalystError("bids response did not match the bids-page shape", path);
  }
  return env.data.data.results;
}
