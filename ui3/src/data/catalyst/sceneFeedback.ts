import { sendSignedJSON, type SendOpts } from "./client";
import type { SendMessageResponse } from "../../generated/catalyst/communities/SendMessageResponse";

export type FeedbackRoute = "message" | "request" | "pending" | "incoming";

type Addressed = { address: string };

function has(list: readonly Addressed[], address: string): boolean {
  const want = address.toLowerCase();
  return list.some((x) => x.address.toLowerCase() === want);
}

export function feedbackRoute(
  owner: string,
  friends: readonly Addressed[],
  sent: readonly Addressed[],
  received: readonly Addressed[],
): FeedbackRoute {
  if (has(friends, owner)) return "message";
  if (has(sent, owner)) return "pending";
  if (has(received, owner)) return "incoming";
  return "request";
}

export async function sendOwnerMessage(
  owner: string,
  text: string,
  opts: SendOpts = {},
): Promise<SendMessageResponse | null> {
  return sendSignedJSON<SendMessageResponse>(
    `/v1/friends/${encodeURIComponent(owner.toLowerCase())}/messages`,
    { ...opts, method: "POST", body: { body: text } },
  );
}

export const REQUEST_CONFIRM_TIMEOUT_MS = 10_000;
