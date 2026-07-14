import { z } from "zod";

export const CURATION_STATUSES = ["pending", "approved", "rejected"] as const;
export type CurationStatus = (typeof CURATION_STATUSES)[number];

export const CollectionCurationSchema = z.object({
  id: z.string(),
  collection_id: z.string(),
  assignee: z.string().nullable(),
  status: z.enum(CURATION_STATUSES),
  created_at: z.string(),
  updated_at: z.string(),
});

/**
 * `is_programmatic` is required on catalyrst-builder's `ReviewRowOut`
 * (generated-schemas/builder.ts), and `thumbs` is the collection's own image
 * set. Both are facts a curator decides on, so a row that carries neither is
 * dropped rather than reviewed as a hand-made collection with no images.
 */
export const CurationRowSchema = z.object({
  id: z.string(),
  name: z.string(),
  type: z.enum(["standard", "third_party"]),
  isProgrammatic: z.boolean(),
  status: z
    .enum(["under_review", "synced", "unsynced", "loading"])
    .nullable()
    .default(null),
  isApproved: z.boolean(),
  hasReviews: z.boolean(),
  itemCount: z.number().int().nonnegative(),
  owner: z.string().nullable(),
  ownerLabel: z.string().nullable(),
  forumLink: z.string().nullable(),
  createdAt: z.string(),
  dateLabel: z.string(),
  thumbs: z.array(z.string()),
  curation: CollectionCurationSchema.nullable(),
});
export type CurationRow = z.infer<typeof CurationRowSchema>;

export const CommitteeMemberSchema = z.object({
  address: z.string(),
  name: z.string(),
});
export type CommitteeMember = z.infer<typeof CommitteeMemberSchema>;

export const DISPLAY_STATES = [
  "to_review",
  "under_review",
  "approved",
  "rejected",
  "disabled",
] as const;
export type DisplayState = (typeof DISPLAY_STATES)[number];

export function deriveDisplayState(row: {
  isApproved: boolean;
  hasReviews: boolean;
  curation: { status: CurationStatus; assignee: string | null } | null;
}): DisplayState {
  const { isApproved, hasReviews, curation } = row;
  if (isApproved) {
    if (!curation || curation.status === "approved") return "approved";
    if (curation.status === "rejected") return "rejected";
  } else {
    if (!curation && hasReviews) return "disabled";
    if (curation && curation.status === "rejected") return "rejected";
  }
  if (curation && curation.status === "pending" && curation.assignee) {
    return "under_review";
  }
  return "to_review";
}

export const STATUS_FILTERS = [
  "ALL_STATUS",
  "to_review",
  "under_review",
  "approved",
  "rejected",
] as const;
export type StatusFilter = (typeof STATUS_FILTERS)[number];

export const TYPE_FILTERS = ["ALL_TYPES", "standard", "third_party"] as const;
export type TypeFilter = (typeof TYPE_FILTERS)[number];

export function readStatusFilter(raw: string | null | undefined): StatusFilter {
  const v = (raw ?? "").trim();
  return (STATUS_FILTERS as readonly string[]).includes(v)
    ? (v as StatusFilter)
    : "ALL_STATUS";
}

export function readTypeFilter(raw: string | null | undefined): TypeFilter {
  const v = (raw ?? "").trim();
  return (TYPE_FILTERS as readonly string[]).includes(v)
    ? (v as TypeFilter)
    : "ALL_TYPES";
}

export function readAssigneeFilter(
  raw: string | null | undefined,
  youAddress: string,
): string {
  const v = (raw ?? "").trim().toLowerCase();
  if (!v || v === "all") return "all";
  if (v === "me" || v === "you") return youAddress.toLowerCase();
  return v;
}

export function filterRows(
  rows: CurationRow[],
  args: { status: StatusFilter; type: TypeFilter; assignee: string },
): CurationRow[] {
  const { status, type, assignee } = args;
  return rows.filter((r) => {
    if (type !== "ALL_TYPES" && r.type !== type) return false;
    if (status !== "ALL_STATUS" && deriveDisplayState(r) !== status) return false;
    if (assignee !== "all") {
      const a = (r.curation?.assignee ?? "").toLowerCase();
      if (a !== assignee.toLowerCase()) return false;
    }
    return true;
  });
}

export function relativeTime(iso: string, now: number = Date.now()): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "";
  const diff = Math.max(0, now - then);
  const min = Math.floor(diff / 60000);
  if (min < 60) return `${Math.max(1, min)} minute${min === 1 ? "" : "s"} ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr} hour${hr === 1 ? "" : "s"} ago`;
  const day = Math.floor(hr / 24);
  if (day < 7) return `${day} day${day === 1 ? "" : "s"} ago`;
  const wk = Math.floor(day / 7);
  return `${wk} week${wk === 1 ? "" : "s"} ago`;
}

export type BdCurationRow = {
  id: string;
  name: string;
  type: "standard" | "third_party";
  isProgrammatic?: boolean;
  status: string | null;
  count: number;
  owner: string | null;
  curationStatus: DisplayState;
  assignee: string | null;
  assigneeName?: string;
  you?: boolean;
  date: string;
  ago: string;
  forumLink: string | null;
  thumbs: string[];
};

export function toBdRow(
  row: CurationRow,
  committee: { you: CommitteeMember; members: CommitteeMember[] },
  now: number = Date.now(),
): BdCurationRow {
  const assignee = row.curation?.assignee ?? null;
  const member = assignee
    ? committee.members.find((m) => m.address.toLowerCase() === assignee.toLowerCase())
    : undefined;
  const you = !!assignee && assignee.toLowerCase() === committee.you.address.toLowerCase();
  return {
    id: row.id,
    name: row.name,
    type: row.type,
    isProgrammatic: row.isProgrammatic,
    status: row.status,
    count: row.itemCount,
    owner: row.ownerLabel,
    curationStatus: deriveDisplayState(row),
    assignee,
    assigneeName: member?.name ?? (assignee ? `${assignee.slice(0, 6)}\u{2026}${assignee.slice(-4)}` : undefined),
    you,
    date: row.dateLabel,
    ago: relativeTime(row.createdAt, now),
    forumLink: row.forumLink,
    thumbs: row.thumbs,
  };
}
