import { z } from "zod";

import { getJSON } from "../client";
import { CurationCollectionsOutSchema } from "../generated-schemas/builder";
import {
  type CommitteeMember,
  type CommitteeRow,
} from "./curate-committee";

export type CommitteeData = {
  committee: { you: CommitteeMember; members: CommitteeMember[] };
  allRows: CommitteeRow[];
  isCommittee: boolean;
  usedFixture: boolean;
  error: boolean;
  usedFallback: boolean;
};


/**
 * Rows validate against the generated `CurationCollectionsOutSchema` -- the
 * ts-rs image of what `catalyrst-builder`'s `GET /v1/collections/curation`
 * actually serialises (`handlers/curation.rs` -> `ports/marketplace.rs`).
 *
 * The hand copy this replaces defaulted `is_approved`, `has_reviews`,
 * `is_programmatic` to `false`, `item_count` to `0`, and `committee` /
 * `collections` to `[]`. Every one of those is a curation fact: a queue that
 * failed to parse arrived as "no collections awaiting review, nobody on the
 * committee", and a single drifted row arrived as an unapproved, unreviewed,
 * hand-made collection holding zero items. All of them are required on the
 * Rust structs, so the generated schema simply refuses the payload and
 * `loadCommitteeCuration` reports `error: true`.
 */
const BackendListSchema = CurationCollectionsOutSchema;

type BackendRow = z.infer<typeof BackendListSchema>["collections"][number];
type BackendMember = z.infer<typeof BackendListSchema>["committee"][number];

function toIso(ts: number | null): string {
  if (ts == null) return new Date(0).toISOString();
  return new Date(ts).toISOString();
}

function shortAddr(a: string | null | undefined): string | null {
  if (!a) return null;
  return a.length >= 10 ? `${a.slice(0, 6)}\u{2026}${a.slice(-4)}` : a;
}

function adaptRow(r: BackendRow): CommitteeRow {
  const curation = r.curation
    ? {
        id: r.curation.id,
        collection_id: r.curation.collection_id,
        assignee: r.curation.assignee.toLowerCase(),
        status: r.curation.status,
        created_at: toIso(r.curation.created_at),
        updated_at: toIso(r.curation.updated_at),
      }
    : null;

  const fallbackName =
    r.id.length >= 10 ? `${r.id.slice(0, 6)}\u{2026}${r.id.slice(-4)}` : r.id;

  return {
    id: r.id,
    name: r.name || fallbackName,
    type: r.type,
    isProgrammatic: r.is_programmatic,
    status: r.status,
    isApproved: r.is_approved,
    hasReviews: r.has_reviews,
    itemCount: r.item_count,
    owner: r.owner ? r.owner.toLowerCase() : null,
    ownerLabel: shortAddr(r.owner),
    forumLink: null,
    forumTopicId: null,
    createdAt: toIso(r.created_at),
    dateLabel: r.is_approved ? "Published" : "Review request",
    thumbs: [],
    curation,
    comments: [],
  };
}

function adaptCommittee(
  members: BackendMember[],
  youAddress: string | null,
): { you: CommitteeMember; members: CommitteeMember[] } {
  const ms: CommitteeMember[] = members.map((m) => {
    const address = m.address.toLowerCase();
    return { address, name: m.name?.trim() || shortAddr(address) || address };
  });
  const you = youAddress
    ? (ms.find((m) => m.address === youAddress.toLowerCase()) ?? {
        address: youAddress.toLowerCase(),
        name: shortAddr(youAddress.toLowerCase()) ?? youAddress.toLowerCase(),
      })
    : { address: "", name: "You" };
  return { you, members: ms };
}

export async function loadCommitteeCuration(args: {
  youAddress?: string | null;
  isCommittee: boolean;
  signal?: AbortSignal;
}): Promise<CommitteeData> {
  const { youAddress = null, isCommittee, signal } = args;

  if (!isCommittee) {
    return {
      committee: adaptCommittee([], youAddress),
      allRows: [],
      isCommittee: false,
      usedFixture: false,
      error: false,
      usedFallback: false,
    };
  }

  try {
    const token =
      typeof process !== "undefined"
        ? process.env?.CATALYRST_BUILDER_ADMIN_TOKEN
        : undefined;
    const raw = await getJSON<{ data?: unknown }>(
      "/v1/collections/curation",
      {
        signal,
        headers: token ? { authorization: `Bearer ${token}` } : undefined,
      },
    );
    const body = (raw as { data?: unknown })?.data ?? raw;
    const parsed = BackendListSchema.parse(body);

    const committee = adaptCommittee(parsed.committee, youAddress);
    const allRows = parsed.collections.map(adaptRow);

    return {
      committee,
      allRows,
      isCommittee: true,
      usedFixture: false,
      error: false,
      usedFallback: allRows.length === 0,
    };
  } catch {
    return {
      committee: adaptCommittee([], youAddress),
      allRows: [],
      isCommittee: true,
      usedFixture: false,
      error: true,
      usedFallback: true,
    };
  }
}
