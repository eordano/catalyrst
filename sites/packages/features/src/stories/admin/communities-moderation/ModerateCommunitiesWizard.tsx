import { useEffect, useMemo, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import AdCommunitiesModerationPage from "@ui/admin/pages/AdCommunitiesModerationPage";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  COMMUNITY_STATUSES,
  matchesStatus,
  parseStatus,
  type CommunityDecision,
  type CommunityModerationCard,
  type CommunityStatus,
} from "@data/lib/catalyst/admin/community-moderation";
import {
  moderateMachine,
  resolveModerateSnapshot,
  slugToState,
  stateToSlug,
  type ModerateStateId,
  type SuspendFn,
  type TrackFn,
} from "./machine";

export type ModerateCommunitiesWizardProps = {
  trackCtx: TrackContext;
  cards: CommunityModerationCard[];
  initialStep?: string;
  suspend?: SuspendFn;
  track?: TrackFn;
};

export default function ModerateCommunitiesWizard({
  trackCtx,
  cards,
  initialStep,
  suspend,
  track,
}: ModerateCommunitiesWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <ModerateCommunitiesInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      cards={cards}
      suspend={suspend}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ModerateStateId;
  trackCtx: TrackContext;
  cards: CommunityModerationCard[];
  suspend?: SuspendFn;
  track?: TrackFn;
};

function ModerateCommunitiesInner({
  stateId,
  trackCtx,
  cards,
  suspend,
  track,
}: InnerProps) {
  const [searchParams, setSearchParams] = useSearchParams();

  const search = searchParams.get("search")?.trim() ?? "";
  const status = parseStatus(searchParams.get("status"));

  const seedCommunityId = cards[0]?.id ?? "sample-community";

  const snapshot = useRef(
    resolveModerateSnapshot({
      step: stateId,
      trackCtx,
      suspend,
      track,
      total: cards.length,
      statusFilter: status,
      communityId: seedCommunityId,
    }),
  ).current;

  const [state, send] = useMachine(moderateMachine, {
    input: { trackCtx, suspend, track, total: cards.length },
    snapshot,
  });

  const value = state.value as ModerateStateId;
  const step = stateToSlug(value);

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step) return params;
        params.set("step", step);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, setSearchParams]);

  const searched = useMemo(() => {
    const q = search.toLowerCase();
    return q ? cards.filter((c) => c.name.toLowerCase().includes(q)) : cards;
  }, [cards, search]);

  const counts = useMemo(() => {
    const out = { all: 0, active: 0, suspended: 0, inactive: 0 } as Record<CommunityStatus, number>;
    for (const s of COMMUNITY_STATUSES) {
      out[s] = searched.filter((c) => matchesStatus(c, s)).length;
    }
    return out;
  }, [searched]);

  const visible = useMemo(
    () => searched.filter((c) => matchesStatus(c, status)),
    [searched, status],
  );

  const activeId = state.context.communityId;
  const activeCard =
    cards.find((c) => c.id === activeId) ??
    (activeId
      ? {
          id: activeId,
          name: activeId,
          owner: "\u{2014}",
          ownerName: null,
          privacy: "public" as const,
          active: true,
          suspended: null,
          membersCount: 0,
          thumbnail: "",
          flaggedReason: "",
          status: "Unknown" as const,
          hue: 280,
        }
      : undefined);

  function setParam(key: string, val: string) {
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (val) params.set(key, val);
        else params.delete(key);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }

  const decision: CommunityDecision = state.context.decision ?? (activeCard?.suspended ? "unsuspend" : "suspend");

  return (
    <AdCommunitiesModerationPage
      step={step}
      value={value}
      cards={visible}
      search={search}
      status={status}
      counts={counts}
      activeCard={activeCard}
      decision={decision}
      error={state.context.error}
      resultSuspended={state.context.result?.suspended}
      onSignIn={() => send({ type: "SIGN_IN" })}
      onSearch={(v) => setParam("search", v)}
      onStatus={(s) => {
        setParam("status", s === "all" ? "" : s);
        const total = searched.filter((c) => matchesStatus(c, s)).length;
        send({ type: "SET_FILTER", status: s, total });
      }}
      onReview={(id) => send({ type: "OPEN", communityId: id })}
      onBack={() => send({ type: "CLOSE" })}
      onDecide={(d, reason) => send({ type: "DECIDE", decision: d, reason })}
      onConfirm={() => send({ type: "CONFIRM" })}
      onCancel={() => send({ type: "CANCEL" })}
      onContinue={() => send({ type: "CONTINUE" })}
    />
  );
}
