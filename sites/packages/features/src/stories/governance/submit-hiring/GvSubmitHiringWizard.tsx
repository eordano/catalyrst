import { useEffect, useMemo, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GovernanceChrome from "@ui/governance/frames/GovernanceChrome";

import "@ui/governance/pages/gvsubmithiring.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  membersOf,
  type HiringRequest,
  type HiringSubmitContext,
  type CommitteeMember,
} from "@data/lib/catalyst/governance/submit-hiring";
import {
  hiringMachine,
  resolveHiringSnapshot,
  slugToState,
  stateToSlug,
  emptyDraft,
  type SubmitFn,
  type TrackFn,
  type HiringDraft,
} from "./machine";

export type GvSubmitHiringWizardProps = {
  trackCtx: TrackContext;
  ctx: HiringSubmitContext;
  initialStep?: string;
  submit?: SubmitFn;
  track?: TrackFn;
};

const MD_TOOLS = ["B", "I", "S", "H", "\u{201C}", "\u{2197}", "</>", "\u{229E}", "\u{21B9}"];

export default function GvSubmitHiringWizard(props: GvSubmitHiringWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return <GvSubmitHiringWizardInner key={stateId} stateId={stateId} {...props} />;
}

type InnerProps = GvSubmitHiringWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function GvSubmitHiringWizardInner({ stateId, trackCtx, ctx, submit, track }: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const request: HiringRequest = ctx.request;

  const seededDraft: HiringDraft =
    stateId === "target"
      ? emptyDraft()
      : {
          committee: ctx.sample.committee,
          address: ctx.sample.address,
          reasons: ctx.sample.reasons,
          evidence: ctx.sample.evidence,
          coAuthors: [],
        };

  const snapshot = useRef(
    resolveHiringSnapshot({
      step: stateId,
      trackCtx,
      request,
      errorCopy: ctx.errors,
      submit,
      track,
      draft: stateId === "target" ? undefined : seededDraft,
    }),
  ).current;

  const [state, send] = useMachine(hiringMachine, {
    input: { trackCtx, request, errorCopy: ctx.errors, submit, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const errors = state.context.errors;
  const draft = state.context.draft;

  const [committee, setCommittee] = useState(draft.committee);
  const [committeeOpen, setCommitteeOpen] = useState(false);
  const [address, setAddress] = useState(draft.address);
  const [memberOpen, setMemberOpen] = useState(false);
  const [reasons, setReasons] = useState(draft.reasons);
  const [evidence, setEvidence] = useState(draft.evidence);
  const [coAuthors, setCoAuthors] = useState<string[]>(draft.coAuthors);

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

  const members: CommitteeMember[] = useMemo(
    () => (committee ? membersOf(ctx.committees, committee) : []),
    [committee, ctx.committees],
  );
  const selectedMember = members.find((m) => m.address === address);

  const addCoAuthor = () =>
    setCoAuthors((list) =>
      list.length >= ctx.schema.coAuthors.max ? list : [...list, ""],
    );
  const removeCoAuthor = (i: number) =>
    setCoAuthors((list) => list.filter((_, idx) => idx !== i));
  const setCoAuthorAt = (i: number, addr: string) =>
    setCoAuthors((list) => list.map((a, idx) => (idx === i ? addr : a)));

  const committeeNameFor = (c: string) => c || ctx.shared.targetPlaceholder;
  const memberDisplay = (m: CommitteeMember | undefined) =>
    m ? m.name : address || ctx.shared.memberPlaceholder;

  return (
    <GovernanceChrome
      active="proposals"
      account={ctx.account.label}
      mana={ctx.account.votingPower.toLocaleString()}
      onTab={() => {}}
    >
      <div className="gvsh" data-step={step}>
        <div className="gvsh__back" />
        <div className="gvsh__col">
          <section className="gvsh__section gvsh__section--title">
            <h1 className="gvsh__h1">{ctx.copy.title}</h1>
          </section>
          <section className="gvsh__section">
            <p className="gvsh__lead">{ctx.copy.description}</p>
          </section>

          {value === "target" && (
            <form
              className="gvsh__form"
              onSubmit={(e) => {
                e.preventDefault();
                send({ type: "SUBMIT_TARGET", committee, address });
              }}
            >
              <section className="gvsh__section">
                <label className="gvsh__label">{ctx.shared.targetTitle}</label>
                <div className="gvsh__ddwrap">
                  <button
                    type="button"
                    className={"gvsh__dropdown" + (committeeOpen ? " is-open" : "")}
                    onClick={() => setCommitteeOpen((o) => !o)}
                    aria-haspopup="listbox"
                    aria-expanded={committeeOpen}
                  >
                    <span className={"gvsh__ddtext" + (committee ? "" : " is-placeholder")}>
                      {committeeNameFor(committee)}
                    </span>
                    <span className="gvsh__ddicon" aria-hidden="true">&#x25BE;</span>
                  </button>
                  {committeeOpen && (
                    <ul className="gvsh__menu" role="listbox">
                      {ctx.committees.map((c) => (
                        <li key={c.name}>
                          <button
                            type="button"
                            className={"gvsh__option" + (c.name === committee ? " is-selected" : "")}
                            onClick={() => {
                              setCommittee(c.name);
                              if (request === "remove") setAddress("");
                              setCommitteeOpen(false);
                            }}
                          >
                            {c.name}
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
                {request === "add" ? (
                  <span className="gvsh__adddetail">{ctx.shared.targetDescription}</span>
                ) : null}
                {errors.committee ? (
                  <span className="gvsh__error">{errors.committee}</span>
                ) : null}
              </section>

              <section className="gvsh__section gvsh__section--tight">
                <label className="gvsh__label">{ctx.copy.addressTitle}</label>
                {request === "add" ? (
                  <>
                    <p className="gvsh__sublabel">{ctx.copy.addressDescription}</p>
                    <input
                      className={"gvsh__input" + (errors.address ? " is-error" : "")}
                      type="text"
                      placeholder={ctx.shared.addressPlaceholder}
                      aria-label="Wallet address"
                      value={address}
                      onChange={(e) => setAddress(e.target.value)}
                    />
                  </>
                ) : (
                  <div className="gvsh__ddwrap">
                    <button
                      type="button"
                      className={"gvsh__dropdown" + (memberOpen ? " is-open" : "")}
                      disabled={!committee}
                      onClick={() => setMemberOpen((o) => !o)}
                      aria-haspopup="listbox"
                      aria-expanded={memberOpen}
                    >
                      <span className={"gvsh__ddtext" + (address ? "" : " is-placeholder")}>
                        {memberDisplay(selectedMember)}
                      </span>
                      <span className="gvsh__ddicon" aria-hidden="true">&#x25BE;</span>
                    </button>
                    {memberOpen && (
                      <ul className="gvsh__menu" role="listbox">
                        {members.length === 0 ? (
                          <li className="gvsh__menuempty">Select a committee first</li>
                        ) : (
                          members.map((m) => (
                            <li key={m.address}>
                              <button
                                type="button"
                                className={
                                  "gvsh__option gvsh__option--member" +
                                  (m.address === address ? " is-selected" : "")
                                }
                                onClick={() => {
                                  setAddress(m.address);
                                  setMemberOpen(false);
                                }}
                              >
                                <span
                                  className="gvsh__memberavatar u-avatar"
                                  style={{ ["--sz" as string]: "22px", ["--hue" as string]: m.hue }}
                                  aria-hidden="true"
                                />
                                {m.name}
                              </button>
                            </li>
                          ))
                        )}
                      </ul>
                    )}
                  </div>
                )}
                {errors.address ? <span className="gvsh__error">{errors.address}</span> : null}
              </section>

              <section className="gvsh__section">
                <button type="submit" className="gvsh__submit">
                  Continue
                </button>
              </section>
            </form>
          )}

          {value === "reasons" && (
            <form
              className="gvsh__form"
              onSubmit={(e) => {
                e.preventDefault();
                send({ type: "SUBMIT_REASONS", reasons, evidence, coAuthors });
              }}
            >
              <section className="gvsh__section gvsh__section--tight">
                <label className="gvsh__label">{ctx.copy.reasonsTitle}</label>
                <p className="gvsh__sublabel">{ctx.copy.reasonsDescription}</p>
                <MarkdownEditor
                  name="reasons"
                  value={reasons}
                  onChange={(v) => setReasons(v)}
                  max={ctx.schema.reasons.max}
                  error={!!errors.reasons}
                />
                {errors.reasons ? <span className="gvsh__error">{errors.reasons}</span> : null}
              </section>

              <section className="gvsh__section gvsh__section--tight">
                <label className="gvsh__label">{ctx.copy.evidenceTitle}</label>
                <p className="gvsh__sublabel">{ctx.copy.evidenceDescription}</p>
                <MarkdownEditor
                  name="evidence"
                  value={evidence}
                  onChange={(v) => setEvidence(v)}
                  max={ctx.schema.evidence.max}
                  error={!!errors.evidence}
                />
                {errors.evidence ? <span className="gvsh__error">{errors.evidence}</span> : null}
              </section>

              <section className="gvsh__section">
                <div className="gvsh__coauthorlabel">
                  <label className="gvsh__label gvsh__label--inline">
                    {ctx.shared.coAuthorsTitle}
                  </label>
                  <sup className="gvsh__optional"> (optional)</sup>
                </div>
                <p className="gvsh__sublabel">{ctx.shared.coAuthorsDescription}</p>
                <div className="gvsh__addresses">
                  {coAuthors.map((c, i) => (
                    <span className="gvsh__coauthor" key={i}>
                      <input
                        type="text"
                        aria-label={`Co-author ${i + 1} address`}
                        placeholder={"0x\u{2026}"}
                        value={c}
                        onChange={(e) => setCoAuthorAt(i, e.target.value)}
                        style={{
                          border: "none",
                          background: "transparent",
                          font: "inherit",
                          color: "inherit",
                          width: 240,
                        }}
                      />
                      <button
                        type="button"
                        className="gvsh__coremove"
                        aria-label="Remove co-author"
                        onClick={() => removeCoAuthor(i)}
                      >
                        &#xD7;
                      </button>
                    </span>
                  ))}
                  {coAuthors.length < ctx.schema.coAuthors.max ? (
                    <button type="button" className="gvsh__coadd" onClick={addCoAuthor}>
                      + Add Co-authors
                    </button>
                  ) : null}
                </div>
              </section>

              <section className="gvsh__section gvsh__submitrow">
                <button
                  type="button"
                  className="gvsh__coadd"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
                <button type="submit" className="gvsh__submit">
                  Review proposal
                </button>
              </section>
            </form>
          )}

          {value === "review" && (
            <div className="gvsh__form">
              <section className="gvsh__section gvsh__section--tight">
                <label className="gvsh__label">{ctx.shared.targetTitle}</label>
                <p className="gvsh__lead">
                  {draft.committee} &#x2014;{" "}
                  {request === "add" ? "add committee member" : "remove committee member"}
                </p>
              </section>
              <section className="gvsh__section gvsh__section--tight">
                <label className="gvsh__label">{ctx.copy.addressTitle}</label>
                <p className="gvsh__lead">{draft.address}</p>
              </section>
              <section className="gvsh__section gvsh__section--tight">
                <label className="gvsh__label">{ctx.copy.reasonsTitle}</label>
                <p className="gvsh__lead" style={{ whiteSpace: "pre-wrap" }}>
                  {draft.reasons}
                </p>
              </section>
              <section className="gvsh__section gvsh__section--tight">
                <label className="gvsh__label">{ctx.copy.evidenceTitle}</label>
                <p className="gvsh__lead" style={{ whiteSpace: "pre-wrap" }}>
                  {draft.evidence}
                </p>
              </section>
              {draft.coAuthors.length > 0 ? (
                <section className="gvsh__section gvsh__section--tight">
                  <label className="gvsh__label">{ctx.shared.coAuthorsTitle}</label>
                  <div className="gvsh__addresses">
                    {draft.coAuthors.map((c, i) => (
                      <span className="gvsh__coauthor" key={i}>{c}</span>
                    ))}
                  </div>
                </section>
              ) : null}
              <section className="gvsh__section gvsh__submitrow">
                <button
                  type="button"
                  className="gvsh__coadd"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
                <button
                  type="button"
                  className="gvsh__submit"
                  onClick={() => send({ type: "CONFIRM" })}
                >
                  {ctx.shared.submitLabel}
                </button>
              </section>
            </div>
          )}

          {value === "submitting" && (
            <section className="gvsh__section">
              <div className="gvsh__loading">
                <span className="gvsh__spinner" aria-hidden="true" />
                Submitting your proposal&#x2026;
              </div>
            </section>
          )}

          {value === "success" && (
            <section className="gvsh__section">
              <div className="gvsh__success" role="status">
                <h1 className="gvsh__h1">{ctx.success.title}</h1>
                <p className="gvsh__lead">
                  Your {request === "add" ? "Add" : "Remove"} Committee Member proposal for{" "}
                  {draft.committee} was created (id {state.context.result?.id}).{" "}
                  {ctx.success.lead}
                </p>
                <p className="gvsh__sublabel">{ctx.success.note}</p>
              </div>
            </section>
          )}

          {value === "error" && (
            <section className="gvsh__section">
              <div className="gvsh__errorbox" role="alert">
                <span className="gvsh__errorlabel">{ctx.submitError}</span>
                <span className="gvsh__errortext">{state.context.error}</span>
              </div>
              <div className="gvsh__submitrow">
                <button
                  type="button"
                  className="gvsh__coadd"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
                <button
                  type="button"
                  className="gvsh__submit"
                  onClick={() => send({ type: "RETRY" })}
                >
                  Retry
                </button>
              </div>
            </section>
          )}
        </div>
      </div>
    </GovernanceChrome>
  );
}

function MarkdownEditor({
  name,
  value,
  onChange,
  max,
  error,
}: {
  name: string;
  value: string;
  onChange: (v: string) => void;
  max: number;
  error?: boolean;
}) {
  return (
    <div className="gvsh__md">
      <div className="gvsh__mdbar">
        <div className="gvsh__mdtools">
          {MD_TOOLS.map((g, i) => (
            <button key={i} type="button" className="gvsh__mdtool" tabIndex={-1} aria-hidden="true">
              {g}
            </button>
          ))}
        </div>
        <div className="gvsh__mdtools gvsh__mdtools--right">
          <button type="button" className="gvsh__mdcmd" tabIndex={-1}>
            Preview
          </button>
        </div>
      </div>
      <textarea
        className={"gvsh__mdarea" + (error ? " is-error" : "")}
        name={name}
        value={value}
        aria-invalid={!!error}
        aria-label={name}
        onChange={(e) => onChange(e.target.value.slice(0, max))}
        rows={6}
      />
      <div className="gvsh__mdmsg">
        ({value.length} out of {max.toLocaleString()} characters)
      </div>
    </div>
  );
}
