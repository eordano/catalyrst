import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GvBidVotingFlow from "@ui/governance/workflows/GvBidVotingFlow";

import type { TrackContext } from "@core/lib/telemetry/track";
import type { BidCard } from "@data/lib/catalyst/governance/vote-bid";
import {
  bidVoteMachine,
  resolveBidVoteSnapshot,
  slugToState,
  stateToSlug,
  BID_CHOICES,
  type CastFn,
  type TrackFn,
} from "./machine";

function uiState(value: string): "default" | "casting" | "error" | "redirect" {
  switch (value) {
    case "casting":
      return "casting";
    case "error":
      return "error";
    case "snapshot":
      return "redirect";
    default:
      return "default";
  }
}

function toUiBids(field: BidCard[]) {
  return field.map((b) => ({
    id: b.id,
    title: b.title,
    budget: b.budget,
    power: b.leadingVP,
    choice: b.leadingChoice,
    current: b.current,
  }));
}

export type BidVotingFlowProps = {
  trackCtx: TrackContext;
  bidId: string;
  field: BidCard[];
  maxErrors: number;
  snapshotUrl?: string;
  retryTimer?: string;
  initialStep?: string;
  cast?: CastFn;
  track?: TrackFn;
};

export default function BidVotingFlow(props: BidVotingFlowProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return <BidVotingFlowInner key={stateId} stateId={stateId} {...props} />;
}

type InnerProps = BidVotingFlowProps & {
  stateId: ReturnType<typeof slugToState>;
};

function BidVotingFlowInner({
  stateId,
  trackCtx,
  bidId,
  field,
  maxErrors,
  snapshotUrl = "#snapshot",
  retryTimer = "30s",
  cast,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const fieldSize = field.length;

  const snapshot = useRef(
    resolveBidVoteSnapshot({ step: stateId, trackCtx, bidId, fieldSize, maxErrors, cast, track }),
  ).current;

  const [state, send] = useMachine(bidVoteMachine, {
    input: { trackCtx, bidId, fieldSize, maxErrors, cast, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const choice = state.context.choice;

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

  const uiBids = toUiBids(field);
  const voteLabel = choice ?? "Yes";

  return (
    <div className="bid-voting-flow" data-step={step}>
      <GvBidVotingFlow
        state={uiState(value)}
        vote={voteLabel}
        retryTimer={retryTimer}
        bids={uiBids}
      />

      <div className="bid-voting-flow__controls" role="group" aria-label="Bid vote controls">
        {value === "review" && (
          <button
            type="button"
            className="bid-voting-flow__btn bid-voting-flow__btn--primary"
            onClick={() => send({ type: "ACKNOWLEDGE" })}
          >
            {`I have reviewed all ${fieldSize} bids`}
          </button>
        )}

        {value === "choosing" && (
          <>
            <div
              className="bid-voting-flow__choices"
              role="radiogroup"
              aria-label="Your choice"
            >
              {BID_CHOICES.map((c) => (
                <button
                  key={c}
                  type="button"
                  role="radio"
                  aria-checked={choice === c}
                  className={
                    "bid-voting-flow__choice" + (choice === c ? " is-selected" : "")
                  }
                  onClick={() => send({ type: "SELECT_CHOICE", choice: c })}
                >
                  {c}
                </button>
              ))}
            </div>
            <button
              type="button"
              className="bid-voting-flow__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back to the field
            </button>
            <button
              type="button"
              className="bid-voting-flow__btn bid-voting-flow__btn--primary"
              disabled={!choice}
              onClick={() => send({ type: "CAST" })}
            >
              {choice ? `Vote "${choice}" anyway` : "Pick a choice"}
            </button>
          </>
        )}

        {value === "error" && (
          <>
            <button
              type="button"
              className="bid-voting-flow__btn bid-voting-flow__btn--primary"
              onClick={() => send({ type: "RETRY" })}
            >
              Retry vote
            </button>
            <button
              type="button"
              className="bid-voting-flow__btn"
              onClick={() => send({ type: "REDIRECT" })}
            >
              Vote on Snapshot instead
            </button>
          </>
        )}

        {value === "snapshot" && (
          <a
            className="bid-voting-flow__btn bid-voting-flow__btn--primary"
            href={snapshotUrl}
            onClick={(e) => {
              if (snapshotUrl === "#snapshot") e.preventDefault();
            }}
          >
            Vote on Snapshot
          </a>
        )}

        {value === "completed" && (
          <p className="bid-voting-flow__done" role="status">
            {`Vote "${voteLabel}" registered (simulated).`}
          </p>
        )}
      </div>
    </div>
  );
}
