export type Option = { code: string; label: string };

export type ReportStatus = "open" | "resolved" | "dismissed" | "actioned";

export const MODERATION_DECISIONS = ["resolve", "dismiss", "action", "reopen"] as const;
export type ModerationDecision = (typeof MODERATION_DECISIONS)[number];

export const MAX_NOTE_LENGTH = 1000;

export type ReportCard = {
  id: string;
  entityId: string | null;
  status: ReportStatus;
  reason: string | null;
  reporter: string;
  reporterShort: string;
  createdLabel: string;
  placeTitle: string;
  placeCoords: string | null;
  placeImage: string | null;
  placeCreator: string | null;
  resolution: string | null;
  notes: string | null;
  resolvedBy: string | null;
  hue: number;
};

export type QueueBuckets = {
  open: ReportCard[];
  resolved: ReportCard[];
  dismissed: ReportCard[];
  actioned: ReportCard[];
};

/**
 * No `authGate`: the moderation console has no client-side gate. Whether the
 * queue is shown at all is the server's answer
 * (`catalyrst-places/src/handlers/admin.rs:41` -> `auth.rs:88-100`), reported
 * by the route loader.
 */
export type ModeratePlacesStateValue =
  | "queue"
  | "reviewReport"
  | "decision"
  | "submitting"
  | "moderated";

export function reasonLabel(reasons: Option[], code: string | null): string {
  if (!code) return "Unspecified";
  const hit = reasons.find((r) => r.code === code);
  if (hit) return hit.label;
  return code.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

export function statusLabel(status: ReportStatus): string {
  switch (status) {
    case "open":
      return "Open";
    case "resolved":
      return "Resolved";
    case "dismissed":
      return "Dismissed";
    case "actioned":
      return "Actioned";
  }
}

export function decisionLabel(decision: ModerationDecision): string {
  switch (decision) {
    case "resolve":
      return "Resolve";
    case "dismiss":
      return "Dismiss";
    case "action":
      return "Action";
    case "reopen":
      return "Reopen";
  }
}
