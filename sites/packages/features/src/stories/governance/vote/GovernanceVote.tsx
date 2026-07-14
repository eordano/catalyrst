import { useState } from "react";
import { useMachine } from "@xstate/react";

import "@ui/governance/workflows/gvvotecastingflow.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  voteMachine,
  type CastFn,
  type TrackFn,
} from "./machine";

export type GovernanceVoteProps = {
  proposalId: string;
  variant: string;
  flags: Record<string, unknown>;
  trackCtx: TrackContext;
  choices?: string[];
  totalVp?: string;
  snapshotUrl?: string;
  castVote?: CastFn;
  track?: TrackFn;
};

const REASON_MAX = 140;

const CloseIcon = () => (
  <svg viewBox="0 0 16 16" width="14" height="14" fill="none" aria-hidden="true">
    <path d="M3 3l10 10M13 3 3 13" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);

const VotingDisabled = () => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width="126"
    height="98"
    fill="none"
    viewBox="0 0 126 98"
    className="gvvc__snapicon"
    aria-hidden="true"
  >
    <path
      fillRule="evenodd"
      clipRule="evenodd"
      d="M14 57H10C4.47715 57 0 61.4772 0 67V88C0 93.5228 4.47715 98 10 98H116C121.523 98 126 93.5228 126 88V67C126 61.4772 121.523 57 116 57H112V84H14V57Z"
      fill="#37333D"
    />
    <path d="M21 10C21 4.47715 25.4772 0 31 0H95C100.523 0 105 4.47715 105 10V77H21V10Z" fill="#B9B7BE" />
    <line x1="49" y1="53.2218" x2="76.2218" y2="26" stroke="white" strokeWidth="11" strokeLinecap="round" />
    <line x1="48.7782" y1="26" x2="76" y2="53.2218" stroke="white" strokeWidth="11" strokeLinecap="round" />
  </svg>
);

const ExternalIcon = () => (
  <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M6 3h7v7" />
    <path d="M13 3 6.5 9.5" />
    <path d="M11 9.5V13H3V5h3.5" />
  </svg>
);

export default function GovernanceVote({
  proposalId,
  variant,
  flags,
  trackCtx,
  choices = ["Yes", "No"],
  totalVp = "12,480",
  snapshotUrl = "#snapshot",
  castVote,
  track,
}: GovernanceVoteProps) {
  const guided = flags.guided === true;

  const [choice, setChoice] = useState<string>(choices[0] ?? "Yes");
  const [reason, setReason] = useState<string>("");

  const [state, send] = useMachine(voteMachine, {
    input: { proposalId, choice, totalVp, trackCtx, guided, castVote, track },
  });

  const inChoosing = state.matches("choosing");
  const inReasoning = state.matches("reasoning");
  const inCasting = state.matches("casting");
  const inError = state.matches("castError");
  const inSnapshot = state.matches("snapshotFallback");
  const inRegistered = state.matches("registered");
  const inDone = state.matches("done");

  const reasonError =
    reason.length > 0 && reason.length < 10
      ? "Rationale is too short"
      : reason.length > REASON_MAX
        ? "Rationale is too large"
        : null;

  if (inChoosing) {
    return (
      <div className="gv-vote" data-variant={variant} data-step="choosing">
        <div className="gvvc__choices" role="radiogroup" aria-label="Vote choices">
          {choices.map((c) => (
            <button
              key={c}
              type="button"
              role="radio"
              aria-checked={choice === c}
              className={"gvvc__statebtn" + (choice === c ? " is-active" : "")}
              onClick={() => setChoice(c)}
            >
              {c}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="gvvc__cast"
          disabled={!choice}
          onClick={() => send({ type: "START" })}
        >
          Cast Vote
        </button>
      </div>
    );
  }

  if (inDone) {
    return (
      <div className="gv-vote" data-variant={variant} data-step="done">
        <p className="gvvc__confirmdesc" role="status">
          Your vote on this proposal has been recorded.
        </p>
      </div>
    );
  }

  return (
    <div className="gvvc" data-variant={variant}>
      <div className="gvvc__backdrop" aria-hidden="true" />
      <div className="gvvc__scrim">
        <div
          className={
            "gvvc__modal" + (inSnapshot || inRegistered ? " gvvc__modal--tiny" : "")
          }
          role="dialog"
          aria-modal="true"
          aria-label="Cast your vote"
        >
          <div className="gvvc__modalinner">
            {(inReasoning || inCasting || inError) && (
              <div className="gvvc__content">
                <div className="gvvc__title">
                  <h2 className="gvvc__heading">
                    Please explain the reasoning behind your vote
                  </h2>
                  <p className="gvvc__choiceline">
                    {`You\u{2019}re about to vote "${choice}" with `}
                    <span className="gvvc__votevp">{totalVp} VP</span>
                  </p>
                </div>

                {guided && (
                  <div className="gvvc__body">
                    <div className="gvvc__reasonarea">
                      <span className="gvvc__subhead">Rationale</span>
                      <div className={"gvvc__textarea" + (reasonError ? " has-error" : "")}>
                        <textarea
                          className="gvvc__textareafield"
                          rows={3}
                          value={reason}
                          disabled={inCasting}
                          placeholder={`I\u{2019}m voting "${choice}" because\u{2026}`}
                          onChange={(e) => setReason(e.target.value)}
                          onBlur={() => {
                            if (reason.length >= 10 && !reasonError) {
                              send({ type: "REASON", reason });
                            }
                          }}
                        />
                      </div>
                      <div className="gvvc__textmeta">
                        <span className={"gvvc__error" + (reasonError ? "" : " is-hidden")}>
                          {reasonError || " "}
                        </span>
                        <span className="gvvc__counter">
                          ({reason.length} out of {REASON_MAX} characters)
                        </span>
                      </div>
                    </div>
                  </div>
                )}

                <div className="gvvc__actions">
                  <button
                    type="button"
                    className={"gvvc__cast" + (inCasting ? " is-loading" : "")}
                    disabled={inCasting || (guided && !!reasonError)}
                    onClick={() =>
                      inError ? send({ type: "RETRY" }) : send({ type: "CAST" })
                    }
                  >
                    {inCasting && <span className="gvvc__btnspinner" aria-hidden="true" />}
                    {inError ? "Retry" : "Cast Vote"}
                  </button>
                </div>

                <div className={"gvvc__errornotice" + (inError ? "" : " is-hidden")}>
                  Failed to cast vote
                </div>
              </div>
            )}

            {inSnapshot && (
              <div className="gvvc__content gvvc__content--snapshot">
                <div className="gvvc__snapcontent">
                  <VotingDisabled />
                  <span className="gvvc__snapheader">Voting not available</span>
                  <div className="gvvc__snapdesc">
                    It looks like we&#x2019;re having issues casting your vote from the
                    Governance dApp.
                  </div>
                  <p className="gvvc__snapsuggestion">
                    You can try voting directly on Snapshot &#x2014; the system we use for
                    decision-making.
                  </p>
                </div>
                <div className="gvvc__snapactions">
                  <a
                    className="gvvc__snapbutton"
                    href={snapshotUrl}
                    target="_blank"
                    rel="noreferrer"
                  >
                    Vote on Snapshot
                    <ExternalIcon />
                  </a>
                </div>
              </div>
            )}

            {inRegistered && (
              <div className="gvvc__content gvvc__content--confirm">
                <button
                  type="button"
                  className="gvvc__close"
                  aria-label="Close"
                  onClick={() => send({ type: "DISMISS" })}
                >
                  <CloseIcon />
                </button>
                <div className="gvvc__confirmtext">
                  <h2 className="gvvc__confirmtitle">Vote Registered</h2>
                  <p className="gvvc__confirmdesc">
                    Thank you for voting! Want to add this proposal to your
                    Watchlist to follow the results?
                  </p>
                </div>
                <div className="gvvc__confirmactions">
                  <button
                    type="button"
                    className="gvvc__cast"
                    onClick={() => send({ type: "SUBSCRIBE" })}
                  >
                    Add to Watchlist
                  </button>
                  <button
                    type="button"
                    className="gvvc__basic"
                    onClick={() => send({ type: "DISMISS" })}
                  >
                    No thanks
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
