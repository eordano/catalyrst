export type Option = { code: string; label: string };

export type ReportStatus = "open" | "resolved" | "dismissed" | "actioned";

export const MODERATION_DECISIONS = ["resolve", "dismiss", "action", "reopen"] as const;
export type ModerationDecision = (typeof MODERATION_DECISIONS)[number];

export const MAX_NOTE_LENGTH = 1000;

type Tone = "ok" | "warn" | "bad" | "brand";

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

const STATUS: Record<ReportStatus, { label: string; tone?: Tone }> = {
  open: { label: "Open", tone: "warn" },
  resolved: { label: "Resolved", tone: "ok" },
  dismissed: { label: "Dismissed" },
  actioned: { label: "Actioned", tone: "brand" },
};

export function statusLabel(status: ReportStatus): string {
  return STATUS[status].label;
}

export function statusTone(status: ReportStatus): Tone | undefined {
  return STATUS[status].tone;
}

const DECISION: Record<ModerationDecision, { label: string; tone?: Tone }> = {
  resolve: { label: "Resolve", tone: "ok" },
  dismiss: { label: "Dismiss" },
  action: { label: "Action", tone: "bad" },
  reopen: { label: "Reopen" },
};

export function decisionLabel(decision: ModerationDecision): string {
  return DECISION[decision].label;
}

export function decisionTone(decision: ModerationDecision): Tone | undefined {
  return DECISION[decision].tone;
}

export function placeArt(card: ReportCard) {
  return card.placeImage
    ? { backgroundImage: `url(${card.placeImage})` }
    : { background: `hsl(${card.hue} 60% 38%)` };
}
