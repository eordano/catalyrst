import { useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GovernanceChrome from "@ui/governance/frames/GovernanceChrome";
import "@ui/governance/pages/gvsubmitcouncildecisionveto.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  COUNCIL_VETO_SCHEMA,
  parseDecisionUrl,
  validateDecisionUrl,
  validateReasons,
  validateSuggestions,
  type CouncilVetoData,
} from "@data/lib/catalyst/governance/submit-council-veto";
import {
  submitCouncilVetoMachine,
  resolveCouncilVetoSnapshot,
  slugToState,
  stateToSlug,
  type CreateFn,
  type TrackFn,
} from "./machine";

export type GvSubmitCouncilVetoWizardProps = {
  trackCtx: TrackContext;
  data: CouncilVetoData;
  initialStep?: string;
  create?: CreateFn;
  track?: TrackFn;
};

export default function GvSubmitCouncilVetoWizard(props: GvSubmitCouncilVetoWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return <GvSubmitCouncilVetoWizardInner key={stateId} stateId={stateId} {...props} />;
}

type InnerProps = GvSubmitCouncilVetoWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

const REASONS_PLACEHOLDER =
  "Explain why this Council decision should be vetoed (markdown). Include the specific issues, inconsistencies or negative impacts you see.";
const SUGGESTIONS_PLACEHOLDER =
  "Optionally, suggest alternative recommendations for the Council (markdown).";

function GvSubmitCouncilVetoWizardInner({ stateId, trackCtx, data, create, track }: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const f = data.fields;

  const isLater = stateId !== "details";
  const seededDetails = isLater
    ? { decisionUrl: data.sample_decision.url }
    : { decisionUrl: "" };
  const seededReasons = isLater
    ? {
        reasons: "This decision should be vetoed because it conflicts with the prior community mandate.",
        suggestions: "",
      }
    : { reasons: "", suggestions: "" };

  const snapshot = useRef(
    resolveCouncilVetoSnapshot({
      step: stateId,
      trackCtx,
      create,
      track,
      details: isLater ? seededDetails : undefined,
      reasons: isLater ? seededReasons : undefined,
      coAuthors: isLater ? [] : undefined,
    }),
  ).current;

  const [state, send] = useMachine(submitCouncilVetoMachine, {
    input: { trackCtx, create, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const ctx = state.context;

  const [decisionUrl, setDecisionUrl] = useState(ctx.details.decisionUrl);
  const [reasons, setReasons] = useState(ctx.reasons.reasons);
  const [suggestions, setSuggestions] = useState(ctx.reasons.suggestions);
  const [coAuthors, setCoAuthors] = useState<string[]>(ctx.coAuthors);

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

  const urlRef = parseDecisionUrl(decisionUrl);
  const urlError =
    decisionUrl.trim() === "" ? "" : validateDecisionUrl(decisionUrl).decision_snapshot_id ?? "";
  const reasonsError = reasons.length === 0 ? "" : validateReasons(reasons).reasons ?? "";
  const suggestionsError = validateSuggestions(suggestions).suggestions ?? "";

  const submitDetails = () => {
    if (decisionUrl.trim() === "" || !parseDecisionUrl(decisionUrl).valid) {
      send({ type: "URL_INVALID" });
      return;
    }
    send({ type: "FILL_DETAILS", decisionUrl });
  };
  const submitReasons = () => {
    if (Object.keys(validateReasons(reasons)).length > 0) return;
    if (Object.keys(validateSuggestions(suggestions)).length > 0) return;
    send({ type: "FILL_REASONS", reasons, suggestions });
  };
  const submitCoAuthors = () => send({ type: "FILL_COAUTHORS", coAuthors });

  const addCoAuthor = () =>
    setCoAuthors((list) =>
      list.length >= COUNCIL_VETO_SCHEMA.coAuthors.max ? list : [...list, `0x${"0".repeat(40)}`],
    );
  const removeCoAuthor = (i: number) =>
    setCoAuthors((list) => list.filter((_, idx) => idx !== i));
  const setCoAuthorAt = (i: number, addr: string) =>
    setCoAuthors((list) => list.map((a, idx) => (idx === i ? addr : a)));

  return (
    <GovernanceChrome active={null} mana={data.account.votingPower.toLocaleString()} account={data.account.label} onTab={() => {}}>
      <div className="gvcdv" data-step={step}>
        <div className="gvcdv__back">
          <button
            type="button"
            className="gvcdv__backbtn"
            aria-label="Back"
            onClick={() => {
              if (value === "details" || value === "submitting") return;
              send({ type: "BACK" });
            }}
          >
            &#x2039;
          </button>
        </div>

        <div className="gvcdv__form">
          <div className="gvcdv__section">
            <h1 className="gvcdv__h1">{data.copy.title}</h1>
          </div>
          <div className="gvcdv__section">
            <p className="gvcdv__lead">{data.copy.description}</p>
          </div>

          {value === "details" && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                submitDetails();
              }}
            >
              <div className="gvcdv__section">
                <label className="gvcdv__label" htmlFor="gvcdv-url">
                  {f.decision_snapshot_id.label}
                </label>
                <p className="gvcdv__sublabel">{f.decision_snapshot_id.detail}</p>
                <input
                  id="gvcdv-url"
                  className={"gvcdv__field" + (urlError ? " is-error" : "")}
                  type="text"
                  value={decisionUrl}
                  onChange={(e) => setDecisionUrl(e.target.value)}
                  placeholder={f.decision_snapshot_id.placeholder ?? ""}
                />
                {urlError ? <p className="gvcdv__mdmessage">{urlError}</p> : null}
                {!urlError && urlRef.valid ? (
                  <p className="gvcdv__addrmsg">Council decision {urlRef.snapshotId.slice(0, 10)}&#x2026;</p>
                ) : null}
              </div>
              <div className="gvcdv__section">
                <button type="submit" className="gvcdv__submit" disabled={!parseDecisionUrl(decisionUrl).valid}>
                  Continue
                </button>
              </div>
            </form>
          )}

          {value === "reasons" && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                submitReasons();
              }}
            >
              <div className="gvcdv__section">
                <label className="gvcdv__label" htmlFor="gvcdv-reasons">
                  {f.reasons.label}
                </label>
                <p className="gvcdv__sublabel">{f.reasons.detail}</p>
                <textarea
                  id="gvcdv-reasons"
                  className={"gvcdv__mdarea" + (reasonsError ? " is-error" : "")}
                  value={reasons}
                  onChange={(e) => setReasons(e.target.value)}
                  placeholder={REASONS_PLACEHOLDER}
                  spellCheck={false}
                />
                <p className="gvcdv__mdmessage">
                  {reasonsError ? <span className="gvcdv__error">{reasonsError} </span> : null}(
                  {reasons.length} out of {COUNCIL_VETO_SCHEMA.reasons.max} characters)
                </p>
              </div>

              <div className="gvcdv__section">
                <div className="gvcdv__labelrow">
                  <label className="gvcdv__label" htmlFor="gvcdv-suggestions">
                    {f.suggestions.label}
                  </label>
                  <sup className="gvcdv__optional">{data.optional_tooltip}</sup>
                </div>
                <p className="gvcdv__sublabel">{f.suggestions.detail}</p>
                <textarea
                  id="gvcdv-suggestions"
                  className={"gvcdv__mdarea" + (suggestionsError ? " is-error" : "")}
                  value={suggestions}
                  onChange={(e) => setSuggestions(e.target.value)}
                  placeholder={SUGGESTIONS_PLACEHOLDER}
                  spellCheck={false}
                />
                <p className="gvcdv__mdmessage">
                  {suggestionsError ? <span className="gvcdv__error">{suggestionsError} </span> : null}(
                  {suggestions.length} out of {COUNCIL_VETO_SCHEMA.suggestions.max} characters)
                </p>
              </div>

              <div className="gvcdv__section">
                <button type="button" className="gvcdv__mdtext" onClick={() => send({ type: "BACK" })}>
                  Back
                </button>
                <button
                  type="submit"
                  className="gvcdv__submit"
                  disabled={Object.keys(validateReasons(reasons)).length > 0 || !!suggestionsError}
                >
                  Continue
                </button>
              </div>
            </form>
          )}

          {value === "coauthors" && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                submitCoAuthors();
              }}
            >
              <div className="gvcdv__section">
                <div className="gvcdv__labelrow">
                  <label className="gvcdv__label">{f.coAuthors.label}</label>
                  <sup className="gvcdv__optional">{data.optional_tooltip}</sup>
                </div>
                <p className="gvcdv__sublabel">{f.coAuthors.detail}</p>
                <div className="gvcdv__addrselect">
                  <div className="gvcdv__chips">
                    {coAuthors.map((a, i) => (
                      <span key={i} className="gvcdv__chip">
                        <input
                          className="gvcdv__addrinput"
                          type="text"
                          aria-label={`Co-author ${i + 1} address`}
                          placeholder={f.coAuthors.placeholder ?? "0x\u{2026}"}
                          value={a}
                          onChange={(e) => setCoAuthorAt(i, e.target.value)}
                        />
                        <button
                          type="button"
                          className="gvcdv__chipx"
                          aria-label={`Remove co-author ${i + 1}`}
                          onClick={() => removeCoAuthor(i)}
                        >
                          &#xD7;
                        </button>
                      </span>
                    ))}
                  </div>
                  {coAuthors.length < COUNCIL_VETO_SCHEMA.coAuthors.max ? (
                    <button type="button" className="gvcdv__mdtext" onClick={addCoAuthor}>
                      + Add co-author
                    </button>
                  ) : null}
                </div>
                <p className="gvcdv__addrmsg">
                  {coAuthors.length}/{f.coAuthors.max} Co-authors added
                </p>
              </div>

              <div className="gvcdv__section">
                <button type="button" className="gvcdv__mdtext" onClick={() => send({ type: "BACK" })}>
                  Back
                </button>
                <button type="submit" className="gvcdv__submit">
                  Review proposal
                </button>
              </div>
            </form>
          )}

          {value === "review" && (
            <div>
              <div className="gvcdv__section">
                <label className="gvcdv__label">{f.decision_snapshot_id.label}</label>
                <p className="gvcdv__lead" style={{ wordBreak: "break-all" }}>
                  {ctx.details.decisionUrl}
                </p>
              </div>
              <div className="gvcdv__section">
                <label className="gvcdv__label">{f.reasons.label}</label>
                <p className="gvcdv__lead" style={{ whiteSpace: "pre-wrap" }}>
                  {ctx.reasons.reasons}
                </p>
              </div>
              {ctx.reasons.suggestions.trim() ? (
                <div className="gvcdv__section">
                  <label className="gvcdv__label">{f.suggestions.label}</label>
                  <p className="gvcdv__lead" style={{ whiteSpace: "pre-wrap" }}>
                    {ctx.reasons.suggestions}
                  </p>
                </div>
              ) : null}
              {ctx.coAuthors.length > 0 ? (
                <div className="gvcdv__section">
                  <label className="gvcdv__label">{f.coAuthors.label}</label>
                  <div className="gvcdv__chips">
                    {ctx.coAuthors.map((a, i) => (
                      <span key={i} className="gvcdv__chip">
                        {a}
                      </span>
                    ))}
                  </div>
                </div>
              ) : null}
              <div className="gvcdv__section">
                <button type="button" className="gvcdv__mdtext" onClick={() => send({ type: "BACK" })}>
                  Back
                </button>
                <button type="button" className="gvcdv__submit" onClick={() => send({ type: "SUBMIT" })}>
                  {data.submit_label}
                </button>
              </div>
            </div>
          )}

          {value === "submitting" && (
            <div className="gvcdv__section">
              <p className="gvcdv__lead">Submitting your veto proposal to Snapshot&#x2026;</p>
            </div>
          )}

          {value === "success" && (
            <div className="gvcdv__section" role="status">
              <h1 className="gvcdv__h1">{data.success.title}</h1>
              <p className="gvcdv__lead">{data.success.lead}</p>
              <p className="gvcdv__sublabel">
                Proposal id {ctx.result?.id} &#x2014; {data.success.note}
              </p>
            </div>
          )}

          {value === "error" && (
            <div className="gvcdv__section">
              <div className="gvcdv__error" role="alert">
                <div className="gvcdv__errorhead">
                  <span className="gvcdv__errorlabel">{data.submit_error}</span>
                </div>
                <p className="gvcdv__sublabel">{ctx.error}</p>
              </div>
              <div className="gvcdv__section">
                <button type="button" className="gvcdv__mdtext" onClick={() => send({ type: "BACK" })}>
                  Back
                </button>
                <button type="button" className="gvcdv__submit" onClick={() => send({ type: "RETRY" })}>
                  Retry
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </GovernanceChrome>
  );
}
