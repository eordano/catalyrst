import { useEffect, useMemo, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  bucketReports,
  type ModerationDecision,
  type Option,
  type ReportCard,
  type ReportRow,
} from "@data/lib/catalyst/admin/places-moderation";
import AdModeratePlacesView from "@ui/admin/pages/AdModeratePlacesView";
import {
  moderateMachine,
  resolveModerateSnapshot,
  slugToState,
  stateToSlug,
  type ModerateStateId,
  type ModerateFn,
  type TrackFn,
} from "./machine";

export type ModeratePlacesWizardProps = {
  trackCtx: TrackContext;
  reports: ReportRow[];
  cards: ReportCard[];
  reasons: Option[];
  resolutions: Option[];
  total: number;
  initialStep?: string;
  moderate?: ModerateFn;
  track?: TrackFn;
};

export default function ModeratePlacesWizard(props: ModeratePlacesWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return <ModeratePlacesWizardInner key={stateId} stateId={stateId} {...props} />;
}

type InnerProps = ModeratePlacesWizardProps & { stateId: ModerateStateId };

function ModeratePlacesWizardInner({
  stateId,
  trackCtx,
  reports,
  cards,
  reasons,
  resolutions,
  total,
  moderate,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const buckets = useMemo(() => bucketReports(cards), [cards]);
  const openCount = buckets.open.length;
  const cardById = useMemo(() => {
    const m = new Map<string, ReportCard>();
    for (const c of cards) m.set(c.id, c);
    return m;
  }, [cards]);

  const seedReportId = buckets.open[0]?.id ?? cards[0]?.id ?? reports[0]?.id;

  const snapshot = useRef(
    resolveModerateSnapshot({
      step: stateId,
      trackCtx,
      reports,
      queueOpenCount: openCount,
      queueTotal: total,
      moderate,
      track,
      reportId: seedReportId,
    }),
  ).current;

  const [state, send] = useMachine(moderateMachine, {
    input: {
      trackCtx,
      reports,
      queueOpenCount: openCount,
      queueTotal: total,
      moderate,
      track,
    },
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

  const activeId = state.context.reportId;
  const activeCard = activeId ? cardById.get(activeId) ?? fallbackCard(activeId) : undefined;
  const decision: ModerationDecision = state.context.decision ?? "resolve";

  return (
    <AdModeratePlacesView
      step={step}
      value={value}
      buckets={buckets}
      reasons={reasons}
      resolutions={resolutions}
      total={total}
      activeId={activeId}
      activeCard={activeCard}
      decision={decision}
      disablePlace={state.context.disablePlace ?? false}
      error={state.context.error}
      resultStatus={state.context.result?.report.status}
      resultPlaceDisabled={state.context.result?.placeDisabled}
      onOpen={(reportId) => send({ type: "OPEN", reportId })}
      onClose={() => send({ type: "CLOSE" })}
      onDecide={(d, resolution, notes) =>
        send({ type: "DECIDE", decision: d, resolution, notes })
      }
      onToggleDisable={(disabled) => send({ type: "TOGGLE_DISABLE", disabled })}
      onConfirm={() => send({ type: "CONFIRM" })}
      onCancel={() => send({ type: "CANCEL" })}
      onContinue={() => send({ type: "CONTINUE" })}
    />
  );
}

function fallbackCard(id: string): ReportCard {
  return {
    id,
    entityId: null,
    status: "open",
    reason: null,
    reporter: "unknown",
    reporterShort: "unknown",
    createdLabel: "\u{2014}",
    placeTitle: `Report ${id}`,
    placeCoords: null,
    placeImage: null,
    placeCreator: null,
    resolution: null,
    notes: null,
    resolvedBy: null,
    hue: 280,
  };
}
