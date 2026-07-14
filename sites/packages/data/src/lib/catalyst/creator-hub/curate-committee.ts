import { z } from "zod";

import {
  CollectionCurationSchema,
  CommitteeMemberSchema,
  CURATION_STATUSES,
  deriveDisplayState,
  relativeTime,
  type CommitteeMember,
  type CurationStatus,
  type DisplayState,
} from "../builder/curation";

export {
  CURATION_STATUSES,
  deriveDisplayState,
  relativeTime,
  type CommitteeMember,
  type CurationStatus,
  type DisplayState,
};

export const COMMENT_DECISIONS = ["approved", "rejected"] as const;
export type CommentDecision = (typeof COMMENT_DECISIONS)[number];

export const CurationCommentSchema = z.object({
  id: z.string(),
  collection_id: z.string(),
  author: z.string(),
  authorName: z.string(),
  decision: z.enum(COMMENT_DECISIONS).nullable(),
  raw: z.string(),
  topic_id: z.number().int().nullable(),
  created_at: z.string(),
});
export type CurationComment = z.infer<typeof CurationCommentSchema>;

/**
 * Mirrors `CurationRowSchema` in `../builder/curation`: `isProgrammatic`,
 * `thumbs` and `comments` are curation facts, not blanks to fill in.
 * `is_programmatic` is required on catalyrst-builder's `ReviewRowOut`, so
 * `false` was a claim about how a collection was made; an empty `comments`
 * said a collection had drawn no review remarks, which is the one thing a
 * curator reads this row for.
 */
export const CommitteeRowSchema = z.object({
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
  forumTopicId: z.number().int().nullable().default(null),
  createdAt: z.string(),
  dateLabel: z.string(),
  thumbs: z.array(z.string()),
  curation: CollectionCurationSchema.nullable(),
  comments: z.array(CurationCommentSchema),
});
export type CommitteeRow = z.infer<typeof CommitteeRowSchema>;

export const CommitteeFixtureSchema = z.object({
  committee: z.object({
    you: CommitteeMemberSchema,
    members: z.array(CommitteeMemberSchema),
  }),
  collections: z.array(CommitteeRowSchema),
});

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
  rows: CommitteeRow[],
  args: { status: StatusFilter; type: TypeFilter; assignee: string },
): CommitteeRow[] {
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

export type BdCommitteeRow = {
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
  createdAtMs?: number | null;
  forumLink: string | null;
  forumTopicId: number | null;
  thumbs: string[];
  comments: CurationComment[];
};

export function toBdRow(
  row: CommitteeRow,
  committee: { you: CommitteeMember; members: CommitteeMember[] },
  now: number = Date.now(),
): BdCommitteeRow {
  const assignee = row.curation?.assignee ?? null;
  const member = assignee
    ? committee.members.find((m) => m.address.toLowerCase() === assignee.toLowerCase())
    : undefined;
  const you =
    !!assignee && assignee.toLowerCase() === committee.you.address.toLowerCase();
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
    assigneeName:
      member?.name ??
      (assignee ? `${assignee.slice(0, 6)}\u{2026}${assignee.slice(-4)}` : undefined),
    you,
    date: row.dateLabel,
    ago: relativeTime(row.createdAt, now),
    createdAtMs: Number.isNaN(Date.parse(row.createdAt))
      ? null
      : Date.parse(row.createdAt),
    forumLink: row.forumLink,
    forumTopicId: row.forumTopicId,
    thumbs: row.thumbs,
    comments: row.comments,
  };
}

export type CommentPostBody = {
  raw: string;
  topic_id: number | null;
};

export function buildOptimisticComment(args: {
  collectionId: string;
  author: CommitteeMember;
  decision: CommentDecision | null;
  raw: string;
  topicId: number | null;
  now?: number;
}): CurationComment {
  const { collectionId, author, decision, raw, topicId, now = Date.now() } = args;
  return {
    id: `c-${collectionId.slice(2, 6)}-${now}`,
    collection_id: collectionId,
    author: author.address,
    authorName: author.name,
    decision,
    raw,
    topic_id: topicId,
    created_at: new Date(now).toISOString(),
  };
}
