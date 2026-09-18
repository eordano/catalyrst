import { z } from "zod";

const CURATION_STATUSES = ["pending", "approved", "rejected"] as const;
type CurationStatus = (typeof CURATION_STATUSES)[number];

export const CollectionCurationSchema = z.object({
  id: z.string(),
  collection_id: z.string(),
  assignee: z.string().nullable(),
  status: z.enum(CURATION_STATUSES),
  created_at: z.string(),
  updated_at: z.string(),
});

z.object({
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

export const CommitteeMemberSchema = z.object({
  address: z.string(),
  name: z.string(),
});
export type CommitteeMember = z.infer<typeof CommitteeMemberSchema>;

const DISPLAY_STATES = [
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

