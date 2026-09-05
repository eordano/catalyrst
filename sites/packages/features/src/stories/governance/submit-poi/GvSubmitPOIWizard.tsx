import { useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GovernanceChrome from "@ui/governance/frames/GovernanceChrome";
import "@ui/governance/pages/gvsubmitpoi.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  POI_SCHEMA,
  type PoiRequest,
  type PoiSubmitContext,
} from "@data/lib/catalyst/governance/submit-poi";
import {
  poiMachine,
  resolvePoiSnapshot,
  slugToState,
  stateToSlug,
  emptyDraft,
  type SubmitFn,
  type TrackFn,
  type PoiDraft,
} from "./machine";

export type GvSubmitPOIWizardProps = {
  trackCtx: TrackContext;
  ctx: PoiSubmitContext;
  initialStep?: string;
  submit?: SubmitFn;
  track?: TrackFn;
};

const COORD_PLACEHOLDER = "-150 through 150";
const MARKDOWN_NOTICE =
  "You can format your proposal using markdown! Toggle the preview switch to see how your post will be displayed.";

export default function GvSubmitPOIWizard(props: GvSubmitPOIWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return <GvSubmitPOIWizardInner key={stateId} stateId={stateId} {...props} />;
}

type InnerProps = GvSubmitPOIWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function GvSubmitPOIWizardInner({
  stateId,
  trackCtx,
  ctx,
  submit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const request: PoiRequest = ctx.request;

  const seededDraft: PoiDraft =
    stateId === "coordinates"
      ? emptyDraft()
      : {
          x: String(ctx.sample.x),
          y: String(ctx.sample.y),
          description: ctx.descriptionPlaceholder,
          coAuthors: [],
        };

  const snapshot = useRef(
    resolvePoiSnapshot({
      step: stateId,
      trackCtx,
      request,
      submit,
      track,
      draft: stateId === "coordinates" ? undefined : seededDraft,
    }),
  ).current;

  const [state, send] = useMachine(poiMachine, {
    input: { trackCtx, request, submit, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const errors = state.context.errors;
  const draft = state.context.draft;

  const [x, setX] = useState(draft.x);
  const [y, setY] = useState(draft.y);
  const [description, setDescription] = useState(draft.description);
  const [coAuthors, setCoAuthors] = useState<string[]>(draft.coAuthors);
  const [preview, setPreview] = useState(false);

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

  const variantCopy = {
    title: ctx.title,
    coordinatesLabel: ctx.coordinatesLabel,
    descriptionDetail: ctx.descriptionDetail,
    descriptionPlaceholder: ctx.descriptionPlaceholder,
  };

  const addCoAuthor = () =>
    setCoAuthors((list) =>
      list.length >= POI_SCHEMA.coAuthors.max
        ? list
        : [...list, `0x${"0".repeat(40)}`],
    );
  const removeCoAuthor = (i: number) =>
    setCoAuthors((list) => list.filter((_, idx) => idx !== i));
  const setCoAuthorAt = (i: number, addr: string) =>
    setCoAuthors((list) => list.map((a, idx) => (idx === i ? addr : a)));

  const xNum = x === "" ? null : Number(x);
  const yNum = y === "" ? null : Number(y);
  const xOut =
    xNum !== null && (!Number.isInteger(xNum) || xNum < POI_SCHEMA.x.min || xNum > POI_SCHEMA.x.max);
  const yOut =
    yNum !== null && (!Number.isInteger(yNum) || yNum < POI_SCHEMA.y.min || yNum > POI_SCHEMA.y.max);
  const liveCoordError =
    xOut || yOut ? "These coordinates are outside of the map limits." : errors.x || errors.y || "";

  const descLen = description.trim().length;
  const descTooShort = description.length > 0 && descLen < POI_SCHEMA.description.min;

  return (
    <GovernanceChrome
      active="proposals"
      account={ctx.account.label}
      mana={ctx.account.votingPower == null ? "\u{2014}" : ctx.account.votingPower.toLocaleString()}
      onTab={() => {}}
    >
      <div className="gvpoi" data-step={step}>
        <div className="gvpoi__col">
          <header className="gvpoi__section">
            <h1 className="gvpoi__title">{variantCopy.title}</h1>
          </header>
          <div className="gvpoi__section">
            <p className="gvpoi__intro">{ctx.intro}</p>
          </div>

          {value === "coordinates" && (
            <form
              className="gvpoi__form"
              onSubmit={(e) => {
                e.preventDefault();
                send({ type: "SUBMIT_COORDINATES", x, y });
              }}
            >
              <div className="gvpoi__section">
                <label className="gvpoi__label">
                  <span>{variantCopy.coordinatesLabel}</span>
                </label>
                <p className="gvpoi__sublabel">
                  Enter the X and Y coordinates of the location.
                </p>
                <div className="gvpoi__coords">
                  <input
                    className={"gvpoi__coordinput" + (xOut || errors.x ? " is-error" : "")}
                    type="number"
                    inputMode="numeric"
                    min={POI_SCHEMA.x.min}
                    max={POI_SCHEMA.x.max}
                    placeholder={COORD_PLACEHOLDER}
                    aria-label="X coordinate"
                    value={x}
                    onChange={(e) => setX(e.target.value)}
                  />
                  <input
                    className={"gvpoi__coordinput" + (yOut || errors.y ? " is-error" : "")}
                    type="number"
                    inputMode="numeric"
                    min={POI_SCHEMA.y.min}
                    max={POI_SCHEMA.y.max}
                    placeholder={COORD_PLACEHOLDER}
                    aria-label="Y coordinate"
                    value={y}
                    onChange={(e) => setY(e.target.value)}
                  />
                  {liveCoordError ? (
                    <div className="gvpoi__coorderror">{liveCoordError}</div>
                  ) : null}
                </div>
              </div>
              <div className="gvpoi__submitrow">
                <button type="submit" className="gvpoi__submit">
                  Continue
                </button>
              </div>
            </form>
          )}

          {value === "description" && (
            <form
              className="gvpoi__form"
              onSubmit={(e) => {
                e.preventDefault();
                send({ type: "SUBMIT_DESCRIPTION", description, coAuthors });
              }}
            >
              <div className="gvpoi__section">
                <label className="gvpoi__label">
                  <span>Description</span>
                  <sup className="gvpoi__notice" title={MARKDOWN_NOTICE}>
                    (markdown)
                  </sup>
                </label>
                <p className="gvpoi__sublabel">{variantCopy.descriptionDetail}</p>
                <div className={"gvpoi__mdfield" + (descTooShort || errors.description ? " is-error" : "")}>
                  <div className="gvpoi__mdbar" aria-hidden="true">
                    <button type="button" className="gvpoi__mdbtn" title="Bold">B</button>
                    <button type="button" className="gvpoi__mdbtn" title="Italic"><i>I</i></button>
                    <button type="button" className="gvpoi__mdbtn" title="Link">&#x2197;</button>
                    <button type="button" className="gvpoi__mdbtn" title="List">&#x2022;</button>
                    <span className="gvpoi__mdspacer" />
                    <label className="gvpoi__mdtoggle">
                      <input
                        type="checkbox"
                        checked={preview}
                        onChange={(e) => setPreview(e.target.checked)}
                      />
                      <span className="gvpoi__mdswitch" />
                      Preview
                    </label>
                  </div>
                  <textarea
                    className="gvpoi__textarea"
                    name="description"
                    value={description}
                    placeholder={variantCopy.descriptionPlaceholder}
                    aria-invalid={descTooShort || !!errors.description}
                    onChange={(e) => setDescription(e.target.value)}
                  />
                </div>
                <div className="gvpoi__message">
                  {descTooShort || errors.description ? (
                    <span className="gvpoi__error">
                      {errors.description || "This description is too short."}
                    </span>
                  ) : null}
                  <span>
                    ({description.length} out of {POI_SCHEMA.description.max} characters)
                  </span>
                </div>
              </div>

              <div className="gvpoi__section">
                <label className="gvpoi__label">
                  <span>Co-authors</span>
                  <sup className="gvpoi__optional">(optional)</sup>
                </label>
                <p className="gvpoi__sublabel">
                  If you co-authored this proposal with someone else, add their wallet
                  addresses to acknowledge their work. After you publish, co-authors are
                  asked to confirm or reject; only confirmed co-authors are listed publicly.
                </p>
                <div className="gvpoi__coauthors">
                  {coAuthors.map((c, i) => (
                    <span className="gvpoi__coauthor" key={i}>
                      <input
                        type="text"
                        aria-label={`Co-author ${i + 1} address`}
                        placeholder={"0x\u{2026}"}
                        value={c}
                        onChange={(e) => setCoAuthorAt(i, e.target.value)}
                        style={{ border: "none", background: "transparent", font: "inherit", color: "inherit", width: 240 }}
                      />
                      <button
                        type="button"
                        className="gvpoi__coremove"
                        aria-label="Remove co-author"
                        onClick={() => removeCoAuthor(i)}
                      >
                        &#xD7;
                      </button>
                    </span>
                  ))}
                  {coAuthors.length < POI_SCHEMA.coAuthors.max ? (
                    <button type="button" className="gvpoi__coadd" onClick={addCoAuthor}>
                      + Add co-author
                    </button>
                  ) : null}
                </div>
              </div>

              <div className="gvpoi__submitrow">
                <button
                  type="button"
                  className="gvpoi__coadd"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
                <button type="submit" className="gvpoi__submit">
                  Review proposal
                </button>
              </div>
            </form>
          )}

          {value === "review" && (
            <div className="gvpoi__form">
              <div className="gvpoi__section">
                <label className="gvpoi__label"><span>Coordinates</span></label>
                <p className="gvpoi__intro">
                  {draft.x}, {draft.y} &#x2014; {request === "add" ? "add as POI" : "remove as POI"}
                </p>
              </div>
              <div className="gvpoi__section">
                <label className="gvpoi__label"><span>Description</span></label>
                <p className="gvpoi__intro" style={{ whiteSpace: "pre-wrap" }}>
                  {draft.description}
                </p>
              </div>
              {draft.coAuthors.length > 0 ? (
                <div className="gvpoi__section">
                  <label className="gvpoi__label"><span>Co-authors</span></label>
                  <div className="gvpoi__coauthors">
                    {draft.coAuthors.map((c, i) => (
                      <span className="gvpoi__coauthor" key={i}>{c}</span>
                    ))}
                  </div>
                </div>
              ) : null}
              <div className="gvpoi__submitrow">
                <button
                  type="button"
                  className="gvpoi__coadd"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
                <button
                  type="button"
                  className="gvpoi__submit"
                  onClick={() => send({ type: "CONFIRM" })}
                >
                  Submit proposal
                </button>
              </div>
            </div>
          )}

          {value === "submitting" && (
            <div className="gvpoi__section">
              <div className="gvpoi__loading">
                <span className="gvpoi__spinner" aria-hidden="true" />
                Submitting your proposal&#x2026; (simulated)
              </div>
            </div>
          )}

          {value === "success" && (
            <div className="gvpoi__section">
              <div className="gvpoi__notfound" role="status">
                <h1 className="gvpoi__notfoundtitle">Proposal submitted</h1>
                <p className="gvpoi__notfounddesc">
                  Your {request === "add" ? "Add" : "Remove"} POI proposal for {draft.x},{" "}
                  {draft.y} was created (simulated &#x2014; id {state.context.result?.proposalId}). On
                  the real DAO this would open for voting on Snapshot.
                </p>
              </div>
            </div>
          )}

          {value === "error" && (
            <div className="gvpoi__section">
              <div className="gvpoi__errorbox" role="alert">
                <span className="gvpoi__errorlabel">
                  There was an error while trying to create the proposal, please try again later.
                </span>
                <span className="gvpoi__errortext">{state.context.error}</span>
              </div>
              <div className="gvpoi__submitrow">
                <button
                  type="button"
                  className="gvpoi__coadd"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
                <button
                  type="button"
                  className="gvpoi__submit"
                  onClick={() => send({ type: "RETRY" })}
                >
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
