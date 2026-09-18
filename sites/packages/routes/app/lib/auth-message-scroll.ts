import type { UnverifiableReason } from "./auth-request-params";

type ScrollMetrics = {
  scrollHeight: number;
  scrollTop: number;
  clientHeight: number;
};

const SCROLL_END_TOLERANCE_PX = 2;

export function isScrolledToEnd(metrics: ScrollMetrics): boolean {
  return (
    metrics.scrollHeight - metrics.scrollTop - metrics.clientHeight <= SCROLL_END_TOLERANCE_PX
  );
}

export function gatesOnPayloadReading(unverifiable: UnverifiableReason | null): boolean {
  return unverifiable !== null;
}

export function acknowledgmentBlocked(gatesOnReading: boolean, readToEnd: boolean): boolean {
  return gatesOnReading && !readToEnd;
}
