import { useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GovernanceChrome from "@ui/governance/frames/GovernanceChrome";
import "@ui/governance/pages/GvSubmitPitch";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  PITCH_SCHEMA,
  MARKDOWN_FIELDS,
  type MarkdownField,
  type PitchDetails,
  type PitchSubmitContext,
} from "@data/lib/catalyst/governance/submit-pitch";
import {
  pitchMachine,
  resolvePitchSnapshot,
  slugToState,
  stateToSlug,
  emptyDraft,
  type SubmitFn,
  type TrackFn,
  type PitchDraft,
} from "./machine";

export type GateInput = {
  connected: boolean;
  hasVp: boolean;
};

export type GvSubmitPitchWizardProps = {
  trackCtx: TrackContext;
  ctx: PitchSubmitContext;
  gate: GateInput;
  account?: string;
  initialStep?: string;
  submit?: SubmitFn;
  track?: TrackFn;
};

const COUNTER = (current: number, limit: number) =>
  `(${current} out of ${limit} characters)`;

export default function GvSubmitPitchWizard(props: GvSubmitPitchWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  const gateKey = `${props.gate.connected ? 1 : 0}${props.gate.hasVp ? 1 : 0}`;
  return (
    <GvSubmitPitchWizardInner
      key={`${stateId}:${gateKey}`}
      stateId={stateId}
      {...props}
    />
  );
}

type InnerProps = GvSubmitPitchWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function GvSubmitPitchWizardInner({
  stateId,
  trackCtx,
  ctx,
  gate,
  account,
  submit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const meetsGate = gate.connected && gate.hasVp;
  const votingPower = 0;

  const seededDraft: PitchDraft =
    stateId === "intro" || stateId === "details"
      ? emptyDraft()
      : { ...ctx.sample, coAuthors: [] };

  const snapshot = useRef(
    resolvePitchSnapshot({
      step: stateId,
      trackCtx,
      meetsGate,
      votingPower,
      submit,
      track,
      draft: stateId === "intro" || stateId === "details" ? undefined : seededDraft,
    }),
  ).current;

  const [state, send] = useMachine(pitchMachine, {
    input: { trackCtx, meetsGate, votingPower, submit, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const errors = state.context.errors;
  const draft = state.context.draft;

  const [form, setForm] = useState<PitchDetails>({
    initiative_name: draft.initiative_name,
    problem_statement: draft.problem_statement,
    proposed_solution: draft.proposed_solution,
    target_audience: draft.target_audience,
    relevance: draft.relevance,
  });
  const [coAuthors, setCoAuthors] = useState<string[]>(draft.coAuthors);
  const setField = (k: keyof PitchDetails) => (v: string) =>
    setForm((f) => ({ ...f, [k]: v }));

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

  const addCoAuthor = () =>
    setCoAuthors((list) =>
      list.length >= PITCH_SCHEMA.coAuthors.max
        ? list
        : [...list, `0x${"0".repeat(40)}`],
    );
  const removeCoAuthor = (i: number) =>
    setCoAuthors((list) => list.filter((_, idx) => idx !== i));
  const setCoAuthorAt = (i: number, addr: string) =>
    setCoAuthors((list) => list.map((a, idx) => (idx === i ? addr : a)));

  const fieldCopy = ctx.copy.fields;

  return (
    <GovernanceChrome
      active="proposals"
      account={account}
      signedIn={gate.connected}
      onTab={() => {}}
    >
      <div className="gsp" data-step={step}>
        <form
          className="gsp__form"
          onSubmit={(e) => e.preventDefault()}
        >
          <section className="gsp__section">
            <h1 className="gsp__title">{ctx.copy.title}</h1>
          </section>

          {value === "intro" && (
            <>
              <section className="gsp__section">
                <p className="gsp__lead">{ctx.copy.description}</p>
                <p className="gsp__lead">{ctx.copy.vpNotice}</p>
              </section>
              {meetsGate ? (
                <section className="gsp__section">
                  <p className="gsp__postlabel">
                    {account
                      ? `Connected as ${account} \u{2014} your wallet meets the voting power requirement to submit.`
                      : "Your wallet meets the voting power requirement to submit."}
                  </p>
                  <button
                    type="button"
                    className="gsp__submit"
                    onClick={() => send({ type: "PASS_GATE" })}
                  >
                    Start your pitch
                  </button>
                </section>
              ) : (
                <section className="gsp__section">
                  <p className="gsp__vpnotice">
                    {gate.connected
                      ? ctx.copy.vpNotice
                      : "Sign in to submit a pitch proposal."}
                  </p>
                  <button type="button" className="gsp__submit" disabled>
                    Start your pitch
                  </button>
                </section>
              )}
            </>
          )}

          {value === "details" && (
            <>
              <section className="gsp__section">
                <p className="gsp__lead">{ctx.copy.description}</p>
              </section>

              <section className="gsp__section">
                <label className="gsp__label">{ctx.copy.initiativeNameLabel}</label>
                <input
                  className={
                    "gsp__input" + (errors.initiative_name ? " is-error" : "")
                  }
                  type="text"
                  name="initiative_name"
                  value={form.initiative_name}
                  maxLength={PITCH_SCHEMA.initiative_name.max}
                  aria-invalid={!!errors.initiative_name}
                  onChange={(e) => setField("initiative_name")(e.target.value)}
                />
                <div
                  className={
                    "gsp__msgrow" + (errors.initiative_name ? " is-error" : "")
                  }
                >
                  {errors.initiative_name ? (
                    <span className="gsp__msg">{errors.initiative_name}</span>
                  ) : (
                    <span className="gsp__msg">
                      {COUNTER(form.initiative_name.length, PITCH_SCHEMA.initiative_name.max)}
                    </span>
                  )}
                </div>
                <p className="gsp__postlabel">{ctx.copy.initiativeNamePostLabel}</p>
              </section>

              {MARKDOWN_FIELDS.map((key: MarkdownField) => {
                const copy = fieldCopy[key];
                const limit = PITCH_SCHEMA[key].max;
                const fieldErr = errors[key];
                return (
                  <section className="gsp__section" key={key}>
                    <label className="gsp__label">
                      {copy.label}
                      <sup className="gsp__markdown" title="You can format using markdown!">
                        {" "}
                        (markdown)
                      </sup>
                    </label>
                    <p className="gsp__sublabel">{copy.detail}</p>
                    <div className={"gsp__md" + (fieldErr ? " is-error" : "")}>
                      <div className="gsp__mdtoolbar" aria-hidden="true">
                        <div className="gsp__mdcmds">
                          {["B", "I", "S", "H", "\u{2022}", "1.", "\u{1F517}", "</>"].map((c, i) => (
                            <button
                              key={i}
                              type="button"
                              className="gsp__mdcmd"
                              tabIndex={-1}
                            >
                              {c}
                            </button>
                          ))}
                        </div>
                        <div className="gsp__mdextra">
                          <button type="button" className="gsp__mdtoggle" tabIndex={-1}>
                            Preview
                          </button>
                        </div>
                      </div>
                      <textarea
                        className="gsp__mdtext"
                        name={key}
                        rows={6}
                        value={form[key]}
                        placeholder={copy.placeholder}
                        aria-invalid={!!fieldErr}
                        onChange={(e) => setField(key)(e.target.value)}
                      />
                    </div>
                    <div className={"gsp__msgrow" + (fieldErr ? " is-error" : "")}>
                      {fieldErr ? (
                        <span className="gsp__msg">{fieldErr}</span>
                      ) : (
                        <span className="gsp__msg">{COUNTER(form[key].length, limit)}</span>
                      )}
                    </div>
                  </section>
                );
              })}

              <section className="gsp__section">
                <button
                  type="button"
                  className="gsp__submit"
                  onClick={() => send({ type: "SUBMIT_DETAILS", details: form })}
                >
                  Continue
                </button>
              </section>
            </>
          )}

          {value === "coauthors" && (
            <section className="gsp__section">
              <div className="gsp__coauthors">
                <div className="gsp__labelrow">
                  <span className="gsp__label gsp__label--inline">{ctx.copy.coAuthorLabel}</span>
                  <sup className="gsp__optional">(optional)</sup>
                </div>
                <p className="gsp__sublabel">{ctx.copy.coAuthorDescription}</p>
                {coAuthors.map((c, i) => (
                  <div className="gsp__addrselect" key={i}>
                    <input
                      className="gsp__addrinput"
                      type="text"
                      aria-label={`Co-author ${i + 1} address`}
                      placeholder="Add a co-author address"
                      value={c}
                      onChange={(e) => setCoAuthorAt(i, e.target.value)}
                    />
                    <button
                      type="button"
                      className="gsp__mdtoggle"
                      aria-label="Remove co-author"
                      onClick={() => removeCoAuthor(i)}
                    >
                      Remove
                    </button>
                  </div>
                ))}
                {coAuthors.length < PITCH_SCHEMA.coAuthors.max ? (
                  <button type="button" className="gsp__mdtoggle" onClick={addCoAuthor}>
                    + Add a co-author address
                  </button>
                ) : (
                  <p className="gsp__postlabel">
                    You can add at most {PITCH_SCHEMA.coAuthors.max} co-authors.
                  </p>
                )}
              </div>
              <div className="gsp__msgrow" style={{ justifyContent: "space-between" }}>
                <button
                  type="button"
                  className="gsp__mdtoggle"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
                <button
                  type="button"
                  className="gsp__submit"
                  onClick={() => send({ type: "SUBMIT_COAUTHORS", coAuthors })}
                >
                  Review proposal
                </button>
              </div>
            </section>
          )}

          {value === "review" && (
            <>
              <section className="gsp__section">
                <label className="gsp__label">{ctx.copy.initiativeNameLabel}</label>
                <p className="gsp__lead">{draft.initiative_name}</p>
              </section>
              {MARKDOWN_FIELDS.map((key: MarkdownField) => (
                <section className="gsp__section" key={key}>
                  <label className="gsp__label">{fieldCopy[key].label}</label>
                  <p className="gsp__lead" style={{ whiteSpace: "pre-wrap" }}>
                    {draft[key]}
                  </p>
                </section>
              ))}
              {draft.coAuthors.length > 0 ? (
                <section className="gsp__section">
                  <label className="gsp__label">{ctx.copy.coAuthorLabel}</label>
                  <div className="gsp__coauthors">
                    {draft.coAuthors.map((c, i) => (
                      <p className="gsp__postlabel" key={i}>{c}</p>
                    ))}
                  </div>
                </section>
              ) : null}
              <section className="gsp__section" style={{ display: "flex", gap: 12 }}>
                <button
                  type="button"
                  className="gsp__mdtoggle"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
                <button
                  type="button"
                  className="gsp__submit"
                  onClick={() => send({ type: "CONFIRM" })}
                >
                  Submit proposal
                </button>
              </section>
            </>
          )}

          {value === "submitting" && (
            <section className="gsp__section">
              <div className="gsp__loading" role="status" aria-label="Submitting">
                <span className="gsp__spinner" aria-hidden="true" />
                Submitting your pitch&#x2026; (simulated)
              </div>
            </section>
          )}

          {value === "success" && (
            <section className="gsp__section">
              <h1 className="gsp__signintitle">Pitch submitted</h1>
              <p className="gsp__signinsub">
                Your pitch &#x201C;{draft.initiative_name}&#x201D; was created (simulated &#x2014; id{" "}
                {state.context.result?.proposalId}). On the real DAO this would open
                for voting on Snapshot as the first step of the Bidding &amp; Tendering
                pipeline.
              </p>
            </section>
          )}

          {value === "error" && (
            <section className="gsp__section">
              <div className="gsp__error">
                <div className="gsp__errhead">
                  <span className="gsp__errnotice">
                    <span className="gsp__errlabel">
                      There was an error while trying to create the proposal, please try
                      again later.
                    </span>
                  </span>
                </div>
                <div className="gsp__errbody">
                  <pre className="gsp__errpre">{state.context.error}</pre>
                </div>
              </div>
              <div className="gsp__section" style={{ display: "flex", gap: 12 }}>
                <button
                  type="button"
                  className="gsp__mdtoggle"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
                <button
                  type="button"
                  className="gsp__submit"
                  onClick={() => send({ type: "RETRY" })}
                >
                  Retry
                </button>
              </div>
            </section>
          )}
        </form>
      </div>
    </GovernanceChrome>
  );
}
