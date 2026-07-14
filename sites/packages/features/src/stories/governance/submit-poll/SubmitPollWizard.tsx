import { useEffect, useMemo, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GovernanceChrome from "@ui/governance/frames/GovernanceChrome";
import GvSubmitPoll from "@ui/governance/pages/GvSubmitPoll";
import GvProposalDetailSuccessOutcomeScreens from "@ui/governance/components/GvProposalDetailSuccessOutcomeScreens";
import "@ui/governance/pages/gvsubmitpoll.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  submitPollMachine,
  resolveSubmitPollSnapshot,
  slugToState,
  stateToSlug,
  cleanOptions,
  isTitleValid,
  isDescriptionValid,
  areDetailsValid,
  areOptionsValid,
  POLL_LIMITS,
  EMPTY_DRAFT,
  type GateInput,
  type PollDraft,
  type SubmitFn,
  type TrackFn,
} from "./machine";

export type SubmitPollWizardProps = {
  trackCtx: TrackContext;
  gate: GateInput;
  account?: string;
  vpLabel?: string;
  initialDraft?: PollDraft;
  initialStep?: string;
  submitPoll?: SubmitFn;
  track?: TrackFn;
};

export default function SubmitPollWizard(props: SubmitPollWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  const gateKey = `${props.gate.connected ? 1 : 0}${props.gate.hasVp ? 1 : 0}`;
  return (
    <SubmitPollWizardInner
      key={`${stateId}:${gateKey}`}
      stateId={stateId}
      {...props}
    />
  );
}

type InnerProps = SubmitPollWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function SubmitPollWizardInner({
  stateId,
  trackCtx,
  gate,
  account = "0x9f3c\u{2026}7a21",
  vpLabel = "12,480",
  initialDraft,
  submitPoll,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const seedDraft = initialDraft ?? EMPTY_DRAFT;

  const snapshot = useRef(
    resolveSubmitPollSnapshot({
      step: stateId,
      trackCtx,
      gate,
      draft: seedDraft,
      submitPoll,
      track,
    }),
  ).current;

  const [state, send] = useMachine(submitPollMachine, {
    input: { trackCtx, gate, draft: seedDraft, submitPoll, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const draft = state.context.draft;

  const [title, setTitle] = useState(draft.title);
  const [description, setDescription] = useState(draft.description);
  const [options, setOptions] = useState<string[]>(
    draft.options.length >= 2 ? draft.options : ["", ""],
  );
  const [coAuthor, setCoAuthor] = useState("");

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

  const counter = (n: number, limit: number) => `(${n} out of ${limit} characters)`;

  const cleaned = useMemo(() => cleanOptions(options), [options]);

  if (value === "intro") {
    return (
      <div className="submit-poll-wizard" data-step="intro">
        <GvSubmitPoll
          account={account}
          loggedIn={gate.connected}
          vpMet={gate.hasVp}
        />
        <div className="submit-poll-wizard__bar" role="group" aria-label="Start poll">
          <button
            type="button"
            className="gvsubmitpoll__submit"
            onClick={() => send({ type: "NEXT" })}
          >
            {gate.connected && gate.hasVp ? "Start poll" : "Continue (gate)"}
          </button>
          {!gate.connected && (
            <p className="gvsubmitpoll__vpnotice">
              Sign in to submit a community poll.
            </p>
          )}
          {gate.connected && !gate.hasVp && (
            <p className="gvsubmitpoll__vpnotice">
              You don&apos;t meet the Voting Power requirement to submit this poll.
              You need at least 100 VP.
            </p>
          )}
        </div>
      </div>
    );
  }

  return (
    <GovernanceChrome active="proposals" signedIn={gate.connected} account={account} onTab={() => {}}>
      <div className="gvsubmitpoll submit-poll-wizard" data-step={step}>
        <form className="gvsubmitpoll__form" onSubmit={(e) => e.preventDefault()}>
          <section className="gvsubmitpoll__section">
            <h1 className="gvsubmitpoll__h1">Create a community poll</h1>
          </section>

          {value === "details" && (
            <>
              <section className="gvsubmitpoll__section">
                <label className="gvsubmitpoll__label" htmlFor="poll-title">Title</label>
                <p className="gvsubmitpoll__sublabel">
                  The question you would like to ask the community.
                </p>
                <input
                  id="poll-title"
                  className="gvsubmitpoll__input"
                  type="text"
                  placeholder="Enter your question here"
                  value={title}
                  maxLength={POLL_LIMITS.title.max}
                  onChange={(e) => setTitle(e.target.value)}
                />
                <p
                  className={
                    "gvsubmitpoll__message" + (isTitleValid(title) || title === "" ? "" : " is-error")
                  }
                >
                  {counter(title.trim().length, POLL_LIMITS.title.max)}
                </p>
              </section>

              <section className="gvsubmitpoll__section">
                <label className="gvsubmitpoll__label" htmlFor="poll-desc">
                  Description
                  <sup className="gvsubmitpoll__notice">(markdown)</sup>
                </label>
                <p className="gvsubmitpoll__sublabel">
                  A brief description of your question.
                </p>
                <div className="gvsubmitpoll__md">
                  <textarea
                    id="poll-desc"
                    className="gvsubmitpoll__mdarea"
                    placeholder="A brief description of your question."
                    value={description}
                    maxLength={POLL_LIMITS.description.max}
                    onChange={(e) => setDescription(e.target.value)}
                  />
                </div>
                <p
                  className={
                    "gvsubmitpoll__message" +
                    (isDescriptionValid(description) || description === "" ? "" : " is-error")
                  }
                >
                  {counter(description.trim().length, POLL_LIMITS.description.max)}
                </p>
              </section>

              <section className="gvsubmitpoll__section submit-poll-wizard__bar">
                <button
                  type="button"
                  className="gvsubmitpoll__addoption"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
                <button
                  type="submit"
                  className="gvsubmitpoll__submit"
                  disabled={!areDetailsValid({ ...draft, title, description })}
                  onClick={() => {
                    send({ type: "SET_DETAILS", title, description });
                    send({ type: "NEXT" });
                  }}
                >
                  Next: Options
                </button>
              </section>
            </>
          )}

          {value === "options" && (
            <>
              <section className="gvsubmitpoll__section">
                <label className="gvsubmitpoll__label">Options</label>
                <div className="gvsubmitpoll__options">
                  {options.map((opt, i) => (
                    <div key={i} className="gvsubmitpoll__option">
                      <input
                        className="gvsubmitpoll__optinput"
                        type="text"
                        placeholder={"choice " + (i + 1)}
                        value={opt}
                        maxLength={POLL_LIMITS.choice.max}
                        onChange={(e) =>
                          setOptions((cur) =>
                            cur.map((o, j) => (j === i ? e.target.value : o)),
                          )
                        }
                      />
                      <button
                        type="button"
                        className="gvsubmitpoll__optaction"
                        aria-label="Remove option"
                        onClick={() =>
                          setOptions((cur) => {
                            const next = cur.filter((_, j) => j !== i);
                            while (next.length < POLL_LIMITS.choices.min) next.push("");
                            return next;
                          })
                        }
                      >
                        &#xD7;
                      </button>
                    </div>
                  ))}
                  <div className="gvsubmitpoll__option is-locked">
                    <input
                      className="gvsubmitpoll__optinput"
                      type="text"
                      readOnly
                      value="Invalid question/options"
                    />
                  </div>
                  <button
                    type="button"
                    className="gvsubmitpoll__addoption"
                    disabled={options.length >= POLL_LIMITS.choices.max}
                    onClick={() => setOptions((cur) => [...cur, ""])}
                  >
                    Add option
                  </button>
                </div>
                <p className="gvsubmitpoll__message">
                  {cleaned.length} option{cleaned.length === 1 ? "" : "s"} (min{" "}
                  {POLL_LIMITS.choices.min}, max {POLL_LIMITS.choices.max})
                </p>
              </section>

              <section className="gvsubmitpoll__section">
                <div className="gvsubmitpoll__labelrow">
                  <label className="gvsubmitpoll__label">Co-authors</label>
                  <sup className="gvsubmitpoll__optional">(optional)</sup>
                </div>
                <p className="gvsubmitpoll__sublabel">
                  Add co-author wallet addresses to acknowledge their work.
                </p>
                <div className="gvsubmitpoll__coauthors">
                  <input
                    className="gvsubmitpoll__input"
                    type="text"
                    placeholder={"Add co-authors (0x\u{2026} 42 chars)"}
                    value={coAuthor}
                    onChange={(e) => setCoAuthor(e.target.value)}
                  />
                </div>
                {draft.coAuthors.length > 0 && (
                  <p className="gvsubmitpoll__message">
                    {draft.coAuthors.length} co-author(s) added
                  </p>
                )}
              </section>

              <section className="gvsubmitpoll__section submit-poll-wizard__bar">
                <button
                  type="button"
                  className="gvsubmitpoll__addoption"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
                <button
                  type="submit"
                  className="gvsubmitpoll__submit"
                  disabled={
                    !areOptionsValid({
                      ...draft,
                      options,
                      coAuthors: collectCoAuthors(coAuthor, draft.coAuthors),
                    })
                  }
                  onClick={() => {
                    send({
                      type: "SET_OPTIONS",
                      options,
                      coAuthors: collectCoAuthors(coAuthor, draft.coAuthors),
                    });
                    send({ type: "NEXT" });
                  }}
                >
                  Next: Review
                </button>
              </section>
            </>
          )}

          {value === "review" && (
            <>
              <section className="gvsubmitpoll__section">
                <label className="gvsubmitpoll__label">{draft.title}</label>
                <p className="gvsubmitpoll__sublabel" style={{ whiteSpace: "pre-wrap" }}>
                  {draft.description}
                </p>
              </section>
              <section className="gvsubmitpoll__section">
                <label className="gvsubmitpoll__label">Options</label>
                <ul className="submit-poll-wizard__review-list">
                  {cleanOptions(draft.options).map((o, i) => (
                    <li key={i}>{o}</li>
                  ))}
                </ul>
                {draft.coAuthors.length > 0 && (
                  <p className="gvsubmitpoll__sublabel">
                    Co-authors: {draft.coAuthors.join(", ")}
                  </p>
                )}
              </section>
              <section className="gvsubmitpoll__section">
                <p className="gvsubmitpoll__intro">
                  Polls are recorded on Snapshot and tracked by the Aragon-governed
                  DAO. This is a <strong>simulated</strong> submission &#x2014; no on-chain
                  or Snapshot write is performed. The flow, validation, telemetry and
                  outcome screen are real.
                </p>
              </section>
              <section className="gvsubmitpoll__section submit-poll-wizard__bar">
                <button
                  type="button"
                  className="gvsubmitpoll__addoption"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
                <button
                  type="button"
                  className="gvsubmitpoll__submit"
                  onClick={() => send({ type: "SUBMIT" })}
                >
                  Submit proposal (simulated)
                </button>
              </section>
            </>
          )}

          {value === "submitting" && (
            <section className="gvsubmitpoll__section" aria-busy="true">
              <p className="gvsubmitpoll__intro" role="status">
                Submitting your poll&#x2026; (simulated createProposal)
              </p>
            </section>
          )}

          {value === "error" && (
            <section className="gvsubmitpoll__section submit-poll-wizard__bar">
              <p className="gvsubmitpoll__vpnotice" role="alert">
                Submission failed: {state.context.error}
              </p>
              <button
                type="button"
                className="gvsubmitpoll__addoption"
                onClick={() => send({ type: "BACK" })}
              >
                Back
              </button>
              <button
                type="button"
                className="gvsubmitpoll__submit"
                onClick={() => send({ type: "RETRY" })}
              >
                Retry
              </button>
            </section>
          )}
        </form>

        {value === "success" && (
          <div className="submit-poll-wizard__success" data-proposal-ref={state.context.result?.proposalRef}>
            <GvProposalDetailSuccessOutcomeScreens variant="new" />
          </div>
        )}
      </div>
    </GovernanceChrome>
  );
}

function collectCoAuthors(typed: string, existing: string[]): string[] {
  const t = typed.trim();
  if (t.length === POLL_LIMITS.coAuthor.len && !existing.includes(t)) {
    return [...existing, t];
  }
  return existing;
}
