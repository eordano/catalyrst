import type { AllowedMethod, UnverifiableReason } from "./auth-request-params";

// Mirrors the one gate upstream's UnverifiedRequestView keeps for itself (messageReadToEnd): a
// message longer than the block that shows it reveals only its first screen, so an authorization
// buried under a benign opening could be acknowledged unseen. The acknowledgment stays closed until
// the block has been scrolled to its end; a block that does not overflow counts as read.

export type ScrollMetrics = {
  scrollHeight: number;
  scrollTop: number;
  clientHeight: number;
};

// Sub-pixel layout leaves a fraction of a pixel below a block that is scrolled all the way down, so
// the end is measured to the pixel and not to zero.
const SCROLL_END_TOLERANCE_PX = 1;

export function isScrolledToEnd(metrics: ScrollMetrics): boolean {
  return (
    metrics.scrollHeight - metrics.scrollTop - metrics.clientHeight <= SCROLL_END_TOLERANCE_PX
  );
}

// Which requests the gate applies to, mirroring upstream's `messageText !== null` scoping: only a
// message shown as the text it is can be read to its end. An opaque message is shown as an escaped
// blob and typed data as a pretty-printed structure, and neither is read the way a sentence is, so
// both are held by the acknowledgment alone.
export function gatesOnMessageReading(
  method: AllowedMethod,
  unverifiable: UnverifiableReason | null,
): boolean {
  return method === "personal_sign" && unverifiable === "unverified_message";
}

export function acknowledgmentBlocked(gatesOnReading: boolean, readToEnd: boolean): boolean {
  return gatesOnReading && !readToEnd;
}
