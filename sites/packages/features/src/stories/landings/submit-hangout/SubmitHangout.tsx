import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import StWhatSOnCreateEditHangout from "@ui/web/pages/StWhatSOnCreateEditHangout";

import type { TrackContext } from "@core/lib/telemetry/track";
import { isStepValid, type HangoutDraft } from "@data/lib/catalyst/landings/submit-hangout";
import {
  hangoutMachine,
  emitStepCompleted,
  resolveHangoutSnapshot,
  slugToState,
  stateToSlug,
  FORM_ORDER,
  type HangoutStateId,
  type SubmitFn,
  type TrackFn,
} from "./machine";

export type CategoryOption = { name: string; label: string };

export type SubmitHangoutProps = {
  trackCtx: TrackContext;
  categories: CategoryOption[];
  categoriesError?: boolean;
  mode?: "create" | "edit";
  draft?: HangoutDraft;
  initialStep?: string;
  submit?: SubmitFn;
  track?: TrackFn;
};

export default function SubmitHangout({
  trackCtx,
  categories,
  categoriesError = false,
  mode = "create",
  draft,
  initialStep,
  submit,
  track,
}: SubmitHangoutProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <SubmitHangoutInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      categories={categories}
      categoriesError={categoriesError}
      mode={mode}
      draft={draft}
      submit={submit}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: HangoutStateId;
  trackCtx: TrackContext;
  categories: CategoryOption[];
  categoriesError: boolean;
  mode: "create" | "edit";
  draft?: HangoutDraft;
  submit?: SubmitFn;
  track?: TrackFn;
};

function uiStateFor(value: HangoutStateId): "signin" | "success" | "form" {
  if (value === "signinGate") return "signin";
  if (value === "submitted") return "success";
  return "form";
}

function SubmitHangoutInner({
  stateId,
  trackCtx,
  categories,
  categoriesError,
  mode,
  draft,
  submit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveHangoutSnapshot({ step: stateId, trackCtx, draft, submit, track }),
  ).current;

  const [state, send] = useMachine(hangoutMachine, {
    input: { trackCtx, draft, submit, track },
    snapshot,
  });

  const value = state.value as HangoutStateId;
  const step = stateToSlug(value);
  const d = state.context.draft;
  const emitTrack: TrackFn = track ?? state.context.track;

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

  const isForm = uiStateFor(value) === "form";
  const formStepIndex = FORM_ORDER.indexOf(value);
  const canAdvanceForm = value === "cover" || isStepValid(value, d);

  return (
    <div className="submit-hangout" data-step={step} data-state={value}>
      <StWhatSOnCreateEditHangout mode={mode} state={uiStateFor(value)} />

      <div className="submit-hangout__controls" role="group" aria-label="Hangout wizard">
        {value === "signinGate" && (
          <button
            type="button"
            className="submit-hangout__btn submit-hangout__btn--primary"
            onClick={() => send({ type: "SIGN_IN" })}
          >
            Sign In to continue
          </button>
        )}

        {isForm && (
          <>
            <StepFields
              step={value}
              draft={d}
              categories={categories}
              categoriesError={categoriesError}
              onEdit={(patch) => send({ type: "EDIT", patch })}
            />
            <div className="submit-hangout__nav">
              <button
                type="button"
                className="submit-hangout__btn"
                onClick={() => send({ type: "BACK" })}
              >
                Back
              </button>

              {value === "review" ? (
                <>
                  <button
                    type="button"
                    className="submit-hangout__btn"
                    onClick={() => send({ type: "PREVIEW" })}
                  >
                    Preview
                  </button>
                  <button
                    type="button"
                    className="submit-hangout__btn submit-hangout__btn--primary"
                    onClick={() => send({ type: "SUBMIT" })}
                  >
                    Submit Hangout
                  </button>
                </>
              ) : value === "preview" ? (
                <button
                  type="button"
                  className="submit-hangout__btn submit-hangout__btn--primary"
                  onClick={() => send({ type: "SUBMIT" })}
                >
                  Confirm &amp; Submit
                </button>
              ) : (
                <ForwardButton
                  from={value}
                  enabled={canAdvanceForm}
                  send={send}
                  emitTrack={emitTrack}
                  trackCtx={state.context.trackCtx}
                  nextStep={FORM_ORDER[formStepIndex + 1]}
                />
              )}
            </div>
          </>
        )}

        {value === "submitting" && (
          <p className="submit-hangout__status" role="status">
            Submitting your hangout for review&#x2026; (simulated)
          </p>
        )}

        {state.context.error && value === "review" && (
          <p className="submit-hangout__error" role="alert">
            Submit failed: {state.context.error}. Please try again.
          </p>
        )}
      </div>
    </div>
  );
}

function ForwardButton({
  from,
  enabled,
  send,
  emitTrack,
  trackCtx,
  nextStep,
}: {
  from: HangoutStateId;
  enabled: boolean;
  send: (e: { type: "NEXT" }) => void;
  emitTrack: TrackFn;
  trackCtx: TrackContext;
  nextStep?: HangoutStateId;
}) {
  return (
    <button
      type="button"
      className="submit-hangout__btn submit-hangout__btn--primary"
      disabled={!enabled}
      onClick={() => {
        if (!enabled) return;
        send({ type: "NEXT" });
        if (nextStep) emitStepCompleted(emitTrack, trackCtx, from, nextStep);
      }}
    >
      Continue
    </button>
  );
}

function StepFields({
  step,
  draft,
  categories,
  categoriesError,
  onEdit,
}: {
  step: HangoutStateId;
  draft: HangoutDraft;
  categories: CategoryOption[];
  categoriesError: boolean;
  onEdit: (patch: Partial<HangoutDraft>) => void;
}) {
  if (step === "cover") {
    return (
      <p className="submit-hangout__hint">
        Cover images are optional. Continue to add details.
      </p>
    );
  }

  if (step === "details") {
    return (
      <div className="submit-hangout__fields">
        <label className="submit-hangout__fieldlabel">
          Hangout Name
          <input
            className="submit-hangout__field"
            type="text"
            value={draft.name}
            placeholder="Be as descriptive as you can"
            onChange={(e) => onEdit({ name: e.target.value })}
          />
        </label>
        <label className="submit-hangout__fieldlabel">
          Description
          <textarea
            className="submit-hangout__field"
            rows={2}
            value={draft.description}
            onChange={(e) => onEdit({ description: e.target.value })}
          />
        </label>
        {categories.length > 0 ? (
          <label className="submit-hangout__fieldlabel">
            Category
            <select
              className="submit-hangout__field"
              value={draft.category}
              onChange={(e) => onEdit({ category: e.target.value })}
            >
              <option value="">None</option>
              {categories.map((c) => (
                <option key={c.name} value={c.name}>
                  {c.label}
                </option>
              ))}
            </select>
          </label>
        ) : (
          <p className="submit-hangout__hint" role="status">
            {categoriesError
              ? "Categories couldn't be loaded right now \u{2014} you can continue and pick one later."
              : "No categories are available right now \u{2014} you can continue without one."}
          </p>
        )}
      </div>
    );
  }

  if (step === "location") {
    const isWorld = draft.location === "world";
    return (
      <div className="submit-hangout__fields">
        <label className="submit-hangout__fieldlabel">
          Location Type
          <select
            className="submit-hangout__field"
            value={draft.location}
            onChange={(e) =>
              onEdit({ location: e.target.value === "world" ? "world" : "land" })
            }
          >
            <option value="land">Land</option>
            <option value="world">World</option>
          </select>
        </label>
        {isWorld ? (
          <label className="submit-hangout__fieldlabel">
            World
            <input
              className="submit-hangout__field"
              type="text"
              value={draft.worldName}
              placeholder="my-world.dcl.eth"
              onChange={(e) => onEdit({ worldName: e.target.value })}
            />
          </label>
        ) : (
          <div className="submit-hangout__coords">
            <label className="submit-hangout__fieldlabel">
              X
              <input
                className="submit-hangout__field"
                type="number"
                value={draft.coordX}
                onChange={(e) => onEdit({ coordX: Number(e.target.value) })}
              />
            </label>
            <label className="submit-hangout__fieldlabel">
              Y
              <input
                className="submit-hangout__field"
                type="number"
                value={draft.coordY}
                onChange={(e) => onEdit({ coordY: Number(e.target.value) })}
              />
            </label>
          </div>
        )}
      </div>
    );
  }

  if (step === "schedule") {
    return (
      <div className="submit-hangout__fields">
        <label className="submit-hangout__fieldlabel">
          Date
          <input
            className="submit-hangout__field"
            type="date"
            value={draft.startDate}
            onChange={(e) => onEdit({ startDate: e.target.value })}
          />
        </label>
        <label className="submit-hangout__fieldlabel">
          Start (UTC)
          <input
            className="submit-hangout__field"
            type="time"
            value={draft.startTime}
            onChange={(e) => onEdit({ startTime: e.target.value })}
          />
        </label>
        <label className="submit-hangout__fieldcheck">
          <input
            type="checkbox"
            checked={draft.recurrent}
            onChange={(e) => onEdit({ recurrent: e.target.checked })}
          />
          Repeat Hangout
        </label>
        {draft.recurrent && (
          <label className="submit-hangout__fieldlabel">
            Ends
            <input
              className="submit-hangout__field"
              type="date"
              value={draft.recurrentUntil}
              onChange={(e) => onEdit({ recurrentUntil: e.target.value })}
            />
          </label>
        )}
      </div>
    );
  }

  return (
    <div className="submit-hangout__review">
      <h3 className="submit-hangout__reviewtitle">Review your hangout</h3>
      <dl className="submit-hangout__reviewlist">
        <dt>Name</dt>
        <dd>{draft.name || "\u{2014}"}</dd>
        <dt>When</dt>
        <dd>
          {draft.startDate || "\u{2014}"} {draft.startTime}
          {draft.recurrent ? ` \u{B7} repeats until ${draft.recurrentUntil || "\u{2014}"}` : ""}
        </dd>
        <dt>Where</dt>
        <dd>
          {draft.location === "world"
            ? draft.worldName || "a world"
            : `LAND (${draft.coordX}, ${draft.coordY})`}
        </dd>
        {draft.category && (
          <>
            <dt>Category</dt>
            <dd>{draft.category}</dd>
          </>
        )}
      </dl>
    </div>
  );
}
