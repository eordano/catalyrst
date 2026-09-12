import type { AllowedMethod, UnverifiableReason } from "./auth-request-params";

export type ScrollMetrics = {
  scrollHeight: number;
  scrollTop: number;
  clientHeight: number;
};

const SCROLL_END_TOLERANCE_PX = 1;

export function isScrolledToEnd(metrics: ScrollMetrics): boolean {
  return (
    metrics.scrollHeight - metrics.scrollTop - metrics.clientHeight <= SCROLL_END_TOLERANCE_PX
  );
}

export function gatesOnMessageReading(
  method: AllowedMethod,
  unverifiable: UnverifiableReason | null,
): boolean {
  return method === "personal_sign" && unverifiable === "unverified_message";
}

export function acknowledgmentBlocked(gatesOnReading: boolean, readToEnd: boolean): boolean {
  return gatesOnReading && !readToEnd;
}
