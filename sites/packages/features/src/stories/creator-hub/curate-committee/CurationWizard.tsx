import { useEffect, useMemo, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useNavigate, useSearchParams } from "react-router";

import ChCurateCommitteeView from "@ui/creatorhub/workflows/ChCurateCommitteeView";

import { track as defaultTrack, type TrackContext } from "@core/lib/telemetry/track";
import type {
  BdCommitteeRow,
  CommitteeMember,
  StatusFilter,
  TypeFilter,
} from "@data/lib/catalyst/creator-hub/curate-committee";
import {
  curationMachine,
  resolveCurationSnapshot,
  slugToState,
  stateToSlug,
  type AssignFn,
  type CurationFilters,
  type DecideFn,
  type DecisionStatus,
  type TrackFn,
} from "./machine";

export type CurationWizardProps = {
  rows: BdCommitteeRow[];
  totalCount: number;
  committee: { you: CommitteeMember; members: CommitteeMember[] };
  isCommittee: boolean;
  filters: CurationFilters;
  trackCtx: TrackContext;
  initialStep?: string;
  initialActiveId?: string;
  initialDecision?: DecisionStatus;
  connectedAddress?: string;
  onConnect?: () => void;
  assign?: AssignFn;
  decide?: DecideFn;
  track?: TrackFn;
};

export default function CurationWizard(props: CurationWizardProps) {
  const [searchParams] = useSearchParams();
  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const raw = slugToState(urlStep);
  const stateId =
    raw === "assigning" ? "dashboard" : raw === "deciding" ? "commenting" : raw;

  return <CurationWizardInner stateId={stateId} {...props} />;
}

type InnerProps = CurationWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function CurationWizardInner({
  stateId,
  rows,
  totalCount,
  committee,
  isCommittee,
  filters,
  trackCtx,
  initialActiveId,
  initialDecision,
  connectedAddress,
  onConnect,
  assign,
  decide,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

  const [assignedIds, setAssignedIds] = useState<Set<string>>(() => new Set());
  const youAddress = connectedAddress || committee.you.address;

  const activeRowForSeed = initialActiveId
    ? rows.find((r) => r.id === initialActiveId)
    : undefined;

  const snapshot = useRef(
    resolveCurationSnapshot({
      step: stateId,
      trackCtx,
      count: totalCount,
      youAddress,
      filters,
      activeId: initialActiveId,
      activeTopicId: activeRowForSeed?.forumTopicId ?? null,
      decision: initialDecision,
      assign,
      decide,
      track,
    }),
  ).current;

  const [state, send] = useMachine(curationMachine, {
    input: {
      trackCtx,
      count: totalCount,
      youAddress,
      filters,
      assign,
      decide,
      track,
    },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const activeId = state.context.activeId;
  const activeRow = rows.find((r) => r.id === activeId);
  const draftComment = state.context.comment;

  const displayRows = useMemo(() => {
    if (assignedIds.size === 0) return rows;
    const label = youAddress
      ? `${youAddress.slice(0, 6)}\u{2026}${youAddress.slice(-4)}`
      : "you";
    return rows.map((r) =>
      assignedIds.has(r.id)
        ? { ...r, assignee: youAddress || r.assignee, assigneeName: label, you: true }
        : r,
    );
  }, [rows, assignedIds, youAddress]);

  const emit = track ?? defaultTrack;
  const viewedFired = useRef(false);
  useEffect(() => {
    if (value !== "dashboard" || viewedFired.current) return;
    viewedFired.current = true;
    emit("bd_curation_viewed", { count: totalCount }, trackCtx);
  }, [value, totalCount, emit, trackCtx]);

  const isTransient = value === "assigning" || value === "deciding";
  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (isTransient) return;
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step && (step !== "dashboard" || !params.has("id")))
          return params;
        params.set("step", step);
        if (step === "dashboard") params.delete("id");
        else if (activeId) params.set("id", activeId);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, activeId, setSearchParams, isTransient]);

  function setFilter(patch: Partial<CurationFilters>) {
    const next: CurationFilters = { ...state.context.filters, ...patch };
    send({ type: "FILTER", filters: next });
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        const sync = (key: keyof CurationFilters, all: string) => {
          if (next[key] === all) params.delete(key);
          else params.set(key, String(next[key]));
        };
        sync("status", "ALL_STATUS");
        sync("type", "ALL_TYPES");
        sync("assignee", "all");
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }

  function onDashboardClick(e: React.MouseEvent<HTMLDivElement>) {
    const el = e.target as HTMLElement;

    const assignLink = el.closest<HTMLElement>(".bdcur__assignee .bdcur__link");
    if (assignLink) {
      const row = rowOfElement(assignLink);
      if (!row) return;
      if (!youAddress) {
        onConnect?.();
        return;
      }
      setAssignedIds((prev) => {
        const next = new Set(prev);
        next.add(row.id);
        return next;
      });
      send({ type: "ASSIGN", id: row.id });
      return;
    }

    const dropItem = el.closest<HTMLElement>(".bdcur__dropitem");
    if (dropItem) {
      const label = (dropItem.textContent ?? "").trim();
      const status = STATUS_LABEL_TO_VALUE[label];
      const type = TYPE_LABEL_TO_VALUE[label];
      const assignee = ASSIGNEE_LABEL_TO_VALUE[label];
      if (status) setFilter({ status });
      else if (type) setFilter({ type });
      else if (assignee) setFilter({ assignee });
      return;
    }

    const row = el.closest<HTMLElement>(".bdcur__row");
    if (row && !el.closest(".bdcur__link") && !el.closest("a")) {
      const r = rowOfElement(row);
      if (r) send({ type: "OPEN_REVIEW", id: r.id, topicId: r.forumTopicId });
    }
  }

  function rowOfElement(el: HTMLElement): BdCommitteeRow | undefined {
    const tr = el.closest<HTMLElement>(".bdcur__row");
    if (!tr) return undefined;
    const id = tr.dataset.id;
    return id ? rows.find((r) => r.id === id) : undefined;
  }

  const builderHref = `/create/wearables/collections/${encodeURIComponent(activeId ?? "")}`;

  return (
    <ChCurateCommitteeView
      view={value}
      step={step}
      isCommittee={isCommittee}
      collections={displayRows}
      collectionsKey={`${filters.status}:${filters.type}:${filters.assignee}`}
      filters={{
        status: filters.status,
        type: filters.type,
        assignee:
          filters.assignee !== "all" &&
          filters.assignee.toLowerCase() === youAddress.toLowerCase()
            ? "me"
            : "all",
      }}
      onDashboardClick={onDashboardClick}
      activeRow={activeRow}
      activeId={activeId ?? undefined}
      builderHref={builderHref}
      decision={state.context.decision}
      draftComment={draftComment}
      authorName={committee.you.name}
      error={state.context.error}
      postedComment={state.context.decideResult?.comment}
      onBack={() => send({ type: "BACK" })}
      onDraftReject={() => send({ type: "DRAFT_DECISION", status: "rejected" })}
      onDraftApprove={() => send({ type: "DRAFT_DECISION", status: "approved" })}
      onChangeComment={(v: string) => send({ type: "EDIT_COMMENT", comment: v })}
      onSubmit={() => send({ type: "SUBMIT", comment: draftComment })}
      onBackToQueue={() => navigate("/create/curate")}
    />
  );
}

const STATUS_LABEL_TO_VALUE: Record<string, StatusFilter | undefined> = {
  "All status": "ALL_STATUS",
  "Under Review": "under_review",
  "To Review": "to_review",
  Approved: "approved",
  Rejected: "rejected",
};
const TYPE_LABEL_TO_VALUE: Record<string, TypeFilter | undefined> = {
  "All types": "ALL_TYPES",
  Standard: "standard",
  Linked: "third_party",
};
const ASSIGNEE_LABEL_TO_VALUE: Record<string, string | undefined> = {
  "All assignees": "all",
  "Assigned to me": "me",
};
