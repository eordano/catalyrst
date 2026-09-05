import { useEffect, useRef, type ComponentProps } from "react";
import { useMachine } from "@xstate/react";
import { Link, useSearchParams } from "react-router";

import StWhatSOn from "@ui/web/pages/StWhatSOn";

type StWhatSOnProps = ComponentProps<typeof StWhatSOn>;
type LiveNow = NonNullable<StWhatSOnProps["liveNow"]>;
type AllDays = NonNullable<StWhatSOnProps["allDays"]>;

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  isStepValid,
  THEME_OPTIONS,
  type ScheduleDraft,
  type Schedule,
} from "@data/lib/catalyst/landings/schedules";
import {
  scheduleMachine,
  emitStepCompleted,
  resolveScheduleSnapshot,
  slugToState,
  stateToSlug,
  FORM_ORDER,
  type ScheduleStateId,
  type SubmitFn,
  type TrackFn,
} from "./machine";

export const BACKGROUND_PRESETS: string[][] = [
  ["#FF2D55", "#FF6B00"],
  ["#7B61FF", "#16141A"],
  ["#00D6CE", "#0B6E99"],
  ["#FFB800", "#FF2D78"],
];

export type ScheduleWizardProps = {
  trackCtx: TrackContext;
  schedules: Schedule[];
  source: "live" | "empty" | "error";
  liveNow?: LiveNow;
  allDays?: AllDays;
  dayLabels?: string[];
  scheduleId?: string;
  draft?: ScheduleDraft;
  initialStep?: string;
  submit?: SubmitFn;
  track?: TrackFn;
};

export default function ScheduleWizard({
  trackCtx,
  schedules,
  source,
  liveNow,
  allDays,
  dayLabels,
  scheduleId,
  draft,
  initialStep,
  submit,
  track,
}: ScheduleWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <ScheduleWizardInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      schedules={schedules}
      source={source}
      liveNow={liveNow}
      allDays={allDays}
      dayLabels={dayLabels}
      scheduleId={scheduleId}
      draft={draft}
      submit={submit}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ScheduleStateId;
  trackCtx: TrackContext;
  schedules: Schedule[];
  source: "live" | "empty" | "error";
  liveNow?: LiveNow;
  allDays?: AllDays;
  dayLabels?: string[];
  scheduleId?: string;
  draft?: ScheduleDraft;
  submit?: SubmitFn;
  track?: TrackFn;
};

function ScheduleWizardInner({
  stateId,
  trackCtx,
  schedules,
  source,
  liveNow,
  allDays,
  dayLabels,
  scheduleId,
  draft,
  submit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveScheduleSnapshot({ step: stateId, trackCtx, draft, scheduleId, submit, track }),
  ).current;

  const [state, send] = useMachine(scheduleMachine, {
    input: { trackCtx, draft, scheduleId, submit, track },
    snapshot,
  });

  const value = state.value as ScheduleStateId;
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

  useListView(emitTrack, state.context.trackCtx, schedules.length, source);

  const isForm = value === "basics" || value === "dates" || value === "review";
  const formStepIndex = FORM_ORDER.indexOf(value);
  const canAdvanceForm = isStepValid(value, d);

  const upcoming = schedules.map((s, i) => ({
    id: s.id,
    name: s.name,
    creator:
      s.active === null ? "Status unknown" : s.active ? "Live program" : "Draft",
    time: rangeLabel(s),
    hue: hueFromBackground(s.background, i),
    image: s.image ?? null,
  }));

  const scheduleBrowse = (
    <section className="es-browse" aria-label="Event schedules">
      <div className="es-browse__head">
        <h1 className="es-browse__title">Event Schedules</h1>
      </div>
      {schedules.length > 0 ? (
        <div className="es-browse__grid">
          {schedules.map((s) => (
            <Link
              key={s.id}
              to={`?mode=edit&id=${encodeURIComponent(s.id)}`}
              prefetch="intent"
              className="es-card"
              aria-label={`Edit ${s.name}`}
            >
              <div
                className="es-card__banner"
                style={
                  {
                    "--c0": s.background?.[0] ?? "#7b61ff",
                    "--c1": s.background?.[1] ?? "#16141a",
                  } as React.CSSProperties
                }
              />
              <div className="es-card__body">
                <h2 className="es-card__name">{s.name}</h2>
                <div className="es-card__range">{rangeLabel(s)}</div>
                {s.description && <p className="es-card__desc">{s.description}</p>}
                <span
                  className={
                    "es-card__badge " +
                    (s.active === true
                      ? "es-card__badge--active"
                      : "es-card__badge--draft")
                  }
                >
                  {scheduleStatusLabel(s.active)}
                </span>
              </div>
            </Link>
          ))}
        </div>
      ) : (
        <p className="es-browse__hint">
          {source === "error"
            ? "Couldn\u{2019}t load schedules right now \u{2014} please try again."
            : "No schedules yet \u{2014} create the first one."}
        </p>
      )}
    </section>
  );

  return (
    <div className="event-schedule" data-step={step} data-state={value} data-source={source}>
      <StWhatSOn
        upcoming={upcoming}
        liveNow={liveNow}
        allDays={allDays}
        dayLabels={dayLabels}
        filterBar={scheduleBrowse}
      />

      <div className="event-schedule__controls" role="group" aria-label="Schedule wizard">
        {value === "authGate" && (
          <>
            <p className="event-schedule__hint">
              Creating a schedule requires moderator access. Admin auth is simulated here.
            </p>
            <div className="event-schedule__nav">
              <button
                type="button"
                className="event-schedule__btn event-schedule__btn--primary"
                onClick={() => send({ type: "SIGN_IN" })}
              >
                Continue as moderator (simulated)
              </button>
            </div>
          </>
        )}

        {isForm && (
          <>
            <StepFields
              step={value}
              draft={d}
              onEdit={(patch) => send({ type: "EDIT", patch })}
            />
            <div className="event-schedule__nav">
              <button
                type="button"
                className="event-schedule__btn"
                onClick={() => send({ type: "BACK" })}
              >
                Back
              </button>

              {value === "review" ? (
                <button
                  type="button"
                  className="event-schedule__btn event-schedule__btn--primary"
                  onClick={() => send({ type: "SUBMIT" })}
                >
                  {scheduleId ? "Save changes" : "Create schedule"}
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
          <p className="event-schedule__status" role="status">
            {scheduleId ? "Saving schedule" : "Creating schedule"}&#x2026; (simulated moderator commit)
          </p>
        )}

        {value === "created" && (
          <p className="event-schedule__status" role="status">
            Schedule {scheduleId ? "saved" : "created"} (simulated). Id:{" "}
            {state.context.result?.id ?? "\u{2014}"}
          </p>
        )}

        {state.context.error && value === "review" && (
          <p className="event-schedule__error" role="alert">
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
  from: ScheduleStateId;
  enabled: boolean;
  send: (e: { type: "NEXT" }) => void;
  emitTrack: TrackFn;
  trackCtx: TrackContext;
  nextStep?: ScheduleStateId;
}) {
  return (
    <button
      type="button"
      className="event-schedule__btn event-schedule__btn--primary"
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
  onEdit,
}: {
  step: ScheduleStateId;
  draft: ScheduleDraft;
  onEdit: (patch: Partial<ScheduleDraft>) => void;
}) {
  if (step === "basics") {
    return (
      <div className="event-schedule__fields">
        <label className="event-schedule__fieldlabel">
          Schedule Name
          <input
            className="event-schedule__field"
            type="text"
            maxLength={50}
            value={draft.name}
            placeholder="e.g. Metaverse Fashion Week 2026"
            onChange={(e) => onEdit({ name: e.target.value })}
          />
        </label>
        <label className="event-schedule__fieldlabel">
          Description
          <textarea
            className="event-schedule__field"
            rows={2}
            maxLength={255}
            value={draft.description}
            onChange={(e) => onEdit({ description: e.target.value })}
          />
        </label>
        <label className="event-schedule__fieldlabel">
          Theme
          <select
            className="event-schedule__field"
            value={draft.theme}
            onChange={(e) => onEdit({ theme: e.target.value })}
          >
            {THEME_OPTIONS.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </select>
        </label>
        <div className="event-schedule__fieldlabel">
          Background
          <div className="event-schedule__swatches" role="group" aria-label="Background colors">
            {BACKGROUND_PRESETS.map((preset) => {
              const active = preset.join() === draft.background.join();
              return (
                <button
                  key={preset.join()}
                  type="button"
                  className={"event-schedule__swatch" + (active ? " is-active" : "")}
                  aria-label={`Background ${preset.join(" to ")}`}
                  aria-pressed={active}
                  style={{
                    background: `linear-gradient(135deg, ${preset[0]} 0%, ${preset[1]} 100%)`,
                  }}
                  onClick={() => onEdit({ background: preset })}
                />
              );
            })}
          </div>
        </div>
      </div>
    );
  }

  if (step === "dates") {
    return (
      <div className="event-schedule__fields">
        <label className="event-schedule__fieldlabel">
          Active from
          <input
            className="event-schedule__field"
            type="date"
            value={draft.activeSinceDate}
            onChange={(e) => onEdit({ activeSinceDate: e.target.value })}
          />
        </label>
        <label className="event-schedule__fieldlabel">
          Active until
          <input
            className="event-schedule__field"
            type="date"
            value={draft.activeUntilDate}
            onChange={(e) => onEdit({ activeUntilDate: e.target.value })}
          />
        </label>
        <label className="event-schedule__fieldcheck">
          <input
            type="checkbox"
            checked={draft.active}
            onChange={(e) => onEdit({ active: e.target.checked })}
          />
          Show in the public listing
        </label>
      </div>
    );
  }

  return (
    <div className="event-schedule__review">
      <h3 className="event-schedule__steptitle">Review your schedule</h3>
      <dl className="event-schedule__reviewlist">
        <dt>Name</dt>
        <dd>{draft.name || "\u{2014}"}</dd>
        <dt>Description</dt>
        <dd>{draft.description || "\u{2014}"}</dd>
        <dt>Theme</dt>
        <dd>{THEME_OPTIONS.find((t) => t.value === draft.theme)?.label ?? "Custom"}</dd>
        <dt>Active window</dt>
        <dd>
          {draft.activeSinceDate || "\u{2014}"} &#x2192; {draft.activeUntilDate || "\u{2014}"}
        </dd>
        <dt>Listing</dt>
        <dd>{draft.active ? "Public" : "Hidden (draft)"}</dd>
      </dl>
    </div>
  );
}

function rangeLabel(s: Schedule): string {
  const fmt = (iso: string | null) => {
    if (!iso) return "TBA";
    const dt = new Date(iso);
    if (Number.isNaN(dt.getTime())) return "TBA";
    return dt.toLocaleString("en-US", { month: "short", day: "numeric", timeZone: "UTC" });
  };
  const a = fmt(s.active_since);
  const b = fmt(s.active_until);
  return a === b ? a : `${a} \u{2013} ${b}`;
}

/** Three states, not two: a schedule whose `active` flag was never read must
 *  not be shown as a draft the operator can safely ignore. */
function scheduleStatusLabel(active: boolean | null): string {
  if (active === null) return "Status unknown";
  return active ? "Active" : "Draft";
}

function hueFromBackground(background: string[] | null, index: number): number {
  const hex = background?.[0];
  if (hex && /^#?[0-9a-fA-F]{6}$/.test(hex)) {
    const n = parseInt(hex.replace("#", ""), 16);
    return n % 360;
  }
  return (index * 47) % 360;
}

function useListView(
  track: TrackFn,
  ctx: TrackContext,
  count: number,
  source: "live" | "empty" | "error",
) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track("lp_schedule_list_viewed", { count, source }, ctx);
  }, [track, ctx, count, source]);
}
