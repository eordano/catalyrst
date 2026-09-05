import { useEffect, useRef, useState } from "react";
import type React from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GovernanceChrome from "@ui/governance/frames/GovernanceChrome";
import SubmitProposalForm from "@ui/governance/components/SubmitProposalForm";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  GOVERNANCE_SCHEMA,
  type GovernanceProposalCopy,
  type LinkedDraft,
} from "@data/lib/catalyst/governance/submit-governance-proposal";
import {
  govProposalMachine,
  resolveGovProposalSnapshot,
  slugToState,
  stateToSlug,
  type GovDraft,
  type SubmitFn,
  type TrackFn,
} from "./machine";

export type GovProposalWizardProps = {
  trackCtx: TrackContext;
  copy: GovernanceProposalCopy;
  drafts: LinkedDraft[];
  live: boolean;
  votingPower: number;
  initialStep?: string;
  submit?: SubmitFn;
  track?: TrackFn;
};

export default function GovProposalWizard(props: GovProposalWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = searchParams.get("step")?.trim() || props.initialStep || undefined;
  const stateId = slugToState(urlStep);

  return <GovProposalWizardInner key={stateId} stateId={stateId} {...props} />;
}

type InnerProps = GovProposalWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function GovProposalWizardInner({
  stateId,
  trackCtx,
  copy,
  drafts,
  live,
  votingPower,
  submit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveGovProposalSnapshot({ step: stateId, trackCtx, votingPower, submit, track }),
  ).current;

  const [state, send] = useMachine(govProposalMachine, {
    input: { trackCtx, votingPower, submit, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const ctx = state.context;
  const vpMet = votingPower >= GOVERNANCE_SCHEMA.vpThreshold;

  const [linkedDraftId, setLinkedDraftId] = useState(
    ctx.draft.linkedDraftId || drafts[0]?.id || "",
  );
  const [title, setTitle] = useState(ctx.draft.title);
  const [bodies, setBodies] = useState<Record<string, string>>(ctx.draft.bodies);
  const [coAuthors, setCoAuthors] = useState<string[]>(ctx.draft.coAuthors);

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

  const selectedDraft = drafts.find((d) => d.id === (ctx.draft.linkedDraftId || linkedDraftId));

  const BODY_NAMES = GOVERNANCE_SCHEMA.bodies.map((b) => b.name);
  const detailsSections = [
    {
      type: "select",
      name: "linked_proposal_id",
      label: "Linked Draft",
      sublabel: "The passed Draft proposal this Governance Proposal formalizes.",
      readOnly: drafts.length <= 1,
      value: linkedDraftId,
      options: drafts.map((d) => ({ value: d.id, label: `${d.title} \u{2014} ${d.author_short}` })),
      placeholder: "Select the passed Draft to link",
    },
    {
      type: "text",
      name: "title",
      label: "Title",
      placeholder: "Enter a descriptive title for your proposal here",
      maxLength: GOVERNANCE_SCHEMA.title.max,
      value: title,
    },
    ...GOVERNANCE_SCHEMA.bodies.map((b) => ({
      type: "markdown",
      name: b.name,
      label: b.label,
      sublabel: b.sublabel,
      markdown: true,
      minLength: b.min,
      maxLength: b.max,
      placeholder: "Start typing...",
      value: bodies[b.name] ?? "",
    })),
  ];

  function submitDetails(vals: Record<string, unknown>) {
    const nextLinked = String(vals.linked_proposal_id ?? linkedDraftId ?? "");
    const nextTitle = String(vals.title ?? title ?? "");
    const nextBodies: Record<string, string> = {};
    for (const name of BODY_NAMES) {
      nextBodies[name] = String(vals[name] ?? bodies[name] ?? "");
    }
    setLinkedDraftId(nextLinked);
    setTitle(nextTitle);
    setBodies(nextBodies);
    send({ type: "SUBMIT_DETAILS", linkedDraftId: nextLinked, title: nextTitle, bodies: nextBodies });
  }

  function addCoAuthor() {
    if (coAuthors.length >= GOVERNANCE_SCHEMA.coAuthorsMax) return;
    const rand = () => Math.random().toString(16).slice(2, 6);
    const next = [...coAuthors, `0x${rand()}${rand()}${rand()}${rand()}${rand()}${rand()}${rand()}${rand()}${rand()}${rand()}`.slice(0, 42)];
    setCoAuthors(next);
    send({ type: "SET_COAUTHORS", coAuthors: next });
  }
  function removeCoAuthor(i: number) {
    const next = coAuthors.filter((_, idx) => idx !== i);
    setCoAuthors(next);
    send({ type: "SET_COAUTHORS", coAuthors: next });
  }

  return (
    <GovernanceChrome active="proposals" onTab={() => {}}>
      <div className="govprop-wizard" data-step={step} style={WRAP}>
        <ol className="govprop-wizard__steps" style={STEPS} aria-label="Submission steps">
          {(["intro", "details", "coauthors", "review", "success"] as const).map((s) => (
            <li key={s} style={{ fontWeight: stepActive(step, s) ? 700 : 400, opacity: stepActive(step, s) ? 1 : 0.5 }}>
              {labelFor(s)}
            </li>
          ))}
        </ol>

        {value === "intro" && (
          <section style={CARD}>
            <h1 style={H1}>{copy.title}</h1>
            {copy.description.map((p, i) => (
              <p key={i} style={INTRO}>{p}</p>
            ))}
            <div
              style={{
                ...NOTICE,
                color: vpMet ? "#1f7a3d" : "#b4232a",
                borderColor: vpMet ? "#bfe6cc" : "#f2c7c9",
                background: vpMet ? "#eef9f1" : "#fdf0f1",
              }}
              role="status"
            >
              {vpMet
                ? `You hold ${votingPower.toLocaleString()} VP \u{2014} above the ${copy.vpThreshold.toLocaleString()} VP submission threshold.`
                : `${copy.vpNotice} You hold ${votingPower.toLocaleString()} VP.`}
            </div>

            <label style={LABEL} htmlFor="govprop-linked-draft">Linked Draft</label>
            <p style={SUB}>
              Pick the passed Draft proposal this Governance Proposal formalizes.{" "}
              {live
                ? "Options are live from the DAO governance service."
                : "Live passed Drafts are unavailable right now \u{2014} please try again shortly."}
            </p>
            {drafts.length === 0 ? (
              <p style={{ ...SUB, color: "#b4232a" }} role="status">
                No passed Drafts available to link at the moment.
              </p>
            ) : null}
            <select
              id="govprop-linked-draft"
              style={SELECT}
              value={linkedDraftId}
              onChange={(e) => {
                setLinkedDraftId(e.target.value);
                send({ type: "SUBMIT_DETAILS", linkedDraftId: e.target.value, title, bodies });
              }}
            >
              {drafts.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.title} &#x2014; {d.author_short}
                </option>
              ))}
            </select>

            <div style={CONTROLS}>
              <button
                type="button"
                style={vpMet ? BTN_PRIMARY : BTN_DISABLED}
                disabled={!vpMet}
                onClick={() => send({ type: "START" })}
              >
                {vpMet ? "Start proposal" : `Requires ${copy.vpThreshold.toLocaleString()} VP`}
              </button>
            </div>
          </section>
        )}

        {value === "details" && (
          <section style={CARD}>
            {Object.keys(ctx.errors).length > 0 && (
              <p style={ERR} role="alert">
                Please complete the required fields: {Object.values(ctx.errors).slice(0, 3).join(" ")}
              </p>
            )}
            <SubmitProposalForm
              {...({
                key: "details",
                title: copy.title,
                description: "Formalize the linked Draft: a Title and the seven required markdown bodies (Summary, Abstract, Motivation, Specification, Impacts, Implementation Pathways, Conclusion).",
                sections: detailsSections,
                submitLabel: "Continue to co-authors",
                secondaryLabel: "Back",
                onSecondary: () => send({ type: "BACK" }),
                onSubmit: submitDetails,
              } as unknown as React.ComponentProps<typeof SubmitProposalForm>)}
            />
          </section>
        )}

        {value === "coauthors" && (
          <section style={CARD}>
            <h1 style={H1}>Add co-authors</h1>
            <p style={SUB}>Co-authors must accept the request to be displayed as such on the proposal. Up to {GOVERNANCE_SCHEMA.coAuthorsMax}. (optional)</p>
            <div style={CHIPS}>
              {coAuthors.length === 0 && <span style={{ opacity: 0.6 }}>No co-authors added.</span>}
              {coAuthors.map((c, i) => (
                <span key={i} style={CHIP}>
                  {c}
                  <button type="button" style={CHIP_X} aria-label="Remove co-author" onClick={() => removeCoAuthor(i)}>&#xD7;</button>
                </span>
              ))}
            </div>
            <div style={CONTROLS}>
              <button type="button" style={BTN} onClick={() => send({ type: "BACK" })}>Back</button>
              <button
                type="button"
                style={BTN}
                disabled={coAuthors.length >= GOVERNANCE_SCHEMA.coAuthorsMax}
                onClick={addCoAuthor}
              >
                + Add co-author (sample)
              </button>
              <button type="button" style={BTN_PRIMARY} onClick={() => send({ type: "NEXT" })}>Review</button>
            </div>
          </section>
        )}

        {value === "review" && (
          <section style={CARD}>
            <h1 style={H1}>Review your Governance Proposal</h1>
            <dl style={REVIEW}>
              <dt style={DT}>Linked Draft</dt>
              <dd style={DD}>{selectedDraft ? `${selectedDraft.title} \u{2014} ${selectedDraft.author_short}` : ctx.draft.linkedDraftId || "\u{2014}"}</dd>
              <dt style={DT}>Title</dt>
              <dd style={DD}>{ctx.draft.title || "\u{2014}"}</dd>
              {GOVERNANCE_SCHEMA.bodies.map((b) => {
                const full = (ctx.draft.bodies[b.name] ?? "").trim();
                const preview = full.length > 0 ? full.slice(0, 140) : "\u{2014}";
                return (
                  <div key={b.name}>
                    <dt style={DT}>{b.label}</dt>
                    <dd style={DD}>
                      {preview}
                      {full.length > 140 ? "\u{2026}" : ""}
                    </dd>
                  </div>
                );
              })}
              <dt style={DT}>Co-authors</dt>
              <dd style={DD}>{ctx.draft.coAuthors.length ? ctx.draft.coAuthors.join(", ") : "None"}</dd>
            </dl>
            <p style={NOTE}>
              Submitting posts a real proposal to the DAO's Snapshot space under this server's signing key. It cannot be edited or withdrawn from here.
            </p>
            <div style={CONTROLS}>
              <button type="button" style={BTN} onClick={() => send({ type: "BACK" })}>Back</button>
              <button type="button" style={BTN_PRIMARY} onClick={() => send({ type: "SUBMIT" })}>Submit proposal</button>
            </div>
          </section>
        )}

        {value === "submitting" && (
          <section style={CARD}>
            <h1 style={H1}>Creating your proposal&#x2026;</h1>
            <p style={INTRO}>Signing and posting your proposal to Snapshot.</p>
            <div style={SPINNER} aria-hidden="true" />
          </section>
        )}

        {value === "submitError" && (
          <section style={CARD}>
            <h1 style={H1}>Submission failed</h1>
            <p style={ERR} role="alert">{ctx.error ?? "There was an error while trying to create the proposal, please try again later."}</p>
            <div style={CONTROLS}>
              <button type="button" style={BTN} onClick={() => send({ type: "BACK" })}>Back to review</button>
              <button type="button" style={BTN_PRIMARY} onClick={() => send({ type: "RETRY" })}>Retry</button>
            </div>
          </section>
        )}

        {value === "success" && (
          <section style={CARD}>
            <h1 style={H1}>Proposal created</h1>
            <p style={INTRO}>Your Governance Proposal is now live for the DAO to vote on.</p>
            <p style={CODE}>Proposal ID: {ctx.result?.id ?? "\u{2014}"}</p>
            <p style={NOTE}>
              This proposal is now on Snapshot and cannot be edited or withdrawn from here.
            </p>
          </section>
        )}
      </div>
    </GovernanceChrome>
  );
}

const STEP_ORDER = ["intro", "details", "coauthors", "review", "success"] as const;
function stepActive(current: string, s: (typeof STEP_ORDER)[number]): boolean {
  const order = ["intro", "details", "coauthors", "review", "submitting", "submit-error", "success"];
  const ci = order.indexOf(current);
  const si = order.indexOf(s);
  return ci >= si;
}
function labelFor(s: (typeof STEP_ORDER)[number]): string {
  return { intro: "Intro", details: "Details", coauthors: "Co-authors", review: "Review", success: "Done" }[s];
}

const WRAP: React.CSSProperties = { maxWidth: 880, margin: "0 auto", padding: "24px 24px 64px" };
const STEPS: React.CSSProperties = { display: "flex", gap: 18, listStyle: "none", padding: 0, margin: "0 0 18px", fontSize: 13, color: "var(--gv-ink, #fcfcfc)" };
const CARD: React.CSSProperties = { background: "#fff", border: "1px solid #e7e4ee", borderRadius: 14, padding: 24 };
const H1: React.CSSProperties = { fontSize: 26, margin: "0 0 12px", color: "#16141a" };
const INTRO: React.CSSProperties = { fontSize: 14, lineHeight: 1.6, color: "#42404a", margin: "0 0 12px" };
const NOTICE: React.CSSProperties = { fontSize: 14, fontWeight: 600, padding: "10px 14px", borderRadius: 10, border: "1px solid", margin: "8px 0 18px" };
const LABEL: React.CSSProperties = { display: "block", fontWeight: 700, fontSize: 14, margin: "8px 0 4px", color: "#16141a" };
const SUB: React.CSSProperties = { fontSize: 13, color: "#6b6975", margin: "0 0 8px" };
const SELECT: React.CSSProperties = { width: "100%", padding: "10px 12px", borderRadius: 10, border: "1px solid #d9d5e3", fontSize: 14 };
const CONTROLS: React.CSSProperties = { display: "flex", gap: 10, marginTop: 20, flexWrap: "wrap" };
const BTN: React.CSSProperties = { padding: "10px 18px", borderRadius: 10, border: "1px solid #d9d5e3", background: "#fff", fontWeight: 600, cursor: "pointer" };
const BTN_PRIMARY: React.CSSProperties = { ...BTN, background: "var(--brand-cta)", color: "#fff", border: "1px solid var(--brand)" };
const BTN_DISABLED: React.CSSProperties = { ...BTN, background: "#efedf3", color: "#9b97a6", cursor: "not-allowed" };
const CHIPS: React.CSSProperties = { display: "flex", flexWrap: "wrap", gap: 8, margin: "8px 0" };
const CHIP: React.CSSProperties = { display: "inline-flex", alignItems: "center", gap: 6, padding: "6px 10px", borderRadius: 999, background: "#f3f1f8", fontSize: 13, fontFamily: "monospace" };
const CHIP_X: React.CSSProperties = { border: "none", background: "transparent", cursor: "pointer", fontSize: 16, lineHeight: 1 };
const REVIEW: React.CSSProperties = { margin: "8px 0 0" };
const DT: React.CSSProperties = { fontWeight: 700, fontSize: 13, color: "#16141a", marginTop: 12 };
const DD: React.CSSProperties = { margin: "2px 0 0", fontSize: 14, color: "#42404a" };
const NOTE: React.CSSProperties = { fontSize: 12.5, color: "#8a6d00", background: "#fff8e1", border: "1px solid #f0e0a0", borderRadius: 8, padding: "10px 12px", margin: "16px 0 0" };
const ERR: React.CSSProperties = { fontSize: 13.5, color: "#b4232a", margin: "12px 0 0" };
const CODE: React.CSSProperties = { fontFamily: "monospace", fontSize: 13, background: "#f3f1f8", padding: "8px 12px", borderRadius: 8, display: "inline-block" };
const SPINNER: React.CSSProperties = { width: 28, height: 28, borderRadius: "50%", border: "3px solid #e7e4ee", borderTopColor: "#ff2d55", animation: "spin 0.8s linear infinite", marginTop: 8 };
