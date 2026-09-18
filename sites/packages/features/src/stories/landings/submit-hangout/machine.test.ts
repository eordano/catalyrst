import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  hangoutMachine,
  HANGOUT_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  FORM_ORDER,
  resolveHangoutSnapshot,
  slugToState,
  stateToSlug,
  simulateSubmit,
  type SubmitFn,
  type TrackFn,
} from "./machine";
import { emptyDraft, type HangoutDraft } from "@data/lib/catalyst/landings/submit-hangout";

const RESULT = { id: "local-sim-test", approved: false };

const okSubmit: SubmitFn = async () => RESULT;
const failSubmit: SubmitFn = async () => {
  throw new Error("create endpoint is admin-gated");
};

function validDraft(): HangoutDraft {
  return {
    ...emptyDraft(),
    name: "Neon Nights",
    description: "A live set",
    startDate: "2026-07-18",
    startTime: "20:00",
    location: "land",
    coordX: -45,
    coordY: 120,
  };
}

function inputFor(submit: SubmitFn, track: TrackFn, draft = validDraft()) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "landings-submit-hangout",
      variant: "wizard",
      experimentKey: "lp_hangout_wizard",
    },
    submit,
    track,
    draft,
  };
}

const EXPECTED_STATES = new Set([
  "signinGate",
  "cover",
  "details",
  "location",
  "schedule",
  "review",
  "preview",
  "submitting",
  "submitted",
]);

const TRAVERSAL_EVENTS = [
  { type: "SIGN_IN" as const },
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "PREVIEW" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("hangoutMachine \u{2014} URL ?step slug map", () => {
  it("uses the audit-spec step ids, unique and round-tripping, falling back to signin-gate", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(hangoutMachine.states)));
    expect(mapped).toEqual(EXPECTED_STATES);
    expect(Object.values(STATE_TO_SLUG)).toEqual([
      "signin-gate",
      "cover",
      "details",
      "location",
      "schedule",
      "review",
      "preview",
      "submitting",
      "submitted",
    ]);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.signinGate);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("signinGate");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("hangoutMachine \u{2014} deep-link hydration", () => {
  it("boots signinGate without a snapshot, hydrates submitting silently, and only real transitions track", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const trackCtx = inputFor(submit, track).trackCtx;
    expect(resolveHangoutSnapshot({ step: "signinGate", trackCtx })).toBeUndefined();

    const submitting = createActor(hangoutMachine, {
      input: inputFor(submit, track),
      snapshot: resolveHangoutSnapshot({ step: "submitting", trackCtx, submit, track }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const review = createActor(hangoutMachine, {
      input: inputFor(okSubmit, track),
      snapshot: resolveHangoutSnapshot({ step: "review", trackCtx, track }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    review.send({ type: "PREVIEW" });
    expect(review.getSnapshot().matches("preview")).toBe(true);
    expect(names(track)).toContain(HANGOUT_EVENTS.previewOpened);
  });
});

describe("hangoutMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and review needs SIGN_IN plus the form NEXTs", () => {
    const paths = getShortestPaths(hangoutMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    for (const step of FORM_ORDER) expect(ends.has(step)).toBe(true);
    expect(ends.has("preview")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
    const review = paths.find((p) => (p.state.value as string) === "review");
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toContain("SIGN_IN");
    expect(events.filter((e) => e === "NEXT").length).toBeGreaterThanOrEqual(4);
  });
});

describe("hangoutMachine \u{2014} per-step validation guards", () => {
  it("details without a name and schedule without a date do not advance until edited", () => {
    const details = createActor(hangoutMachine, {
      input: inputFor(okSubmit, () => {}, { ...emptyDraft() }),
    }).start();
    details.send({ type: "SIGN_IN" });
    details.send({ type: "NEXT" });
    expect(details.getSnapshot().matches("details")).toBe(true);
    details.send({ type: "NEXT" });
    expect(details.getSnapshot().matches("details")).toBe(true);
    details.send({ type: "EDIT", patch: { name: "My Hangout" } });
    details.send({ type: "NEXT" });
    expect(details.getSnapshot().matches("location")).toBe(true);

    const draft = { ...validDraft(), startDate: "", startTime: "" };
    const schedule = createActor(hangoutMachine, {
      input: inputFor(okSubmit, () => {}, draft),
      snapshot: resolveHangoutSnapshot({
        step: "schedule",
        trackCtx: inputFor(okSubmit, () => {}).trackCtx,
        draft,
      }),
    }).start();
    schedule.send({ type: "NEXT" });
    expect(schedule.getSnapshot().matches("schedule")).toBe(true);
    schedule.send({ type: "EDIT", patch: { startDate: "2026-07-18", startTime: "20:00" } });
    schedule.send({ type: "NEXT" });
    expect(schedule.getSnapshot().matches("review")).toBe(true);
  });
});

describe("hangoutMachine \u{2014} telemetry (happy path)", () => {
  it("gate -> form steps -> submit -> submitted fires the funnel", async () => {
    const track = vi.fn();
    const actor = createActor(hangoutMachine, {
      input: inputFor(okSubmit, track),
    }).start();
    expect(actor.getSnapshot().matches("signinGate")).toBe(true);
    expect(names(track)).toContain(HANGOUT_EVENTS.gateViewed);

    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitted"));

    const events = names(track);
    for (const e of [
      HANGOUT_EVENTS.gateViewed,
      HANGOUT_EVENTS.started,
      HANGOUT_EVENTS.submitAttempted,
      HANGOUT_EVENTS.submitted,
    ]) {
      expect(events).toContain(e);
    }
    expect(events.indexOf(HANGOUT_EVENTS.submitAttempted)).toBeLessThan(
      events.indexOf(HANGOUT_EVENTS.submitted),
    );
    const startedCall = track.mock.calls.find((c) => c[0] === HANGOUT_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "lp_hangout_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("hangoutMachine \u{2014} submit failure", () => {
  it("submit error returns to review and fires submit_failed", async () => {
    const track = vi.fn();
    const actor = createActor(hangoutMachine, {
      input: inputFor(failSubmit, track),
      snapshot: resolveHangoutSnapshot({
        step: "review",
        trackCtx: inputFor(failSubmit, track).trackCtx,
        submit: failSubmit,
        track,
      }),
    }).start();

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("review"));
    expect(actor.getSnapshot().context.error).toBe("create endpoint is admin-gated");
    const events = names(track);
    expect(events).toContain(HANGOUT_EVENTS.submitAttempted);
    expect(events).toContain(HANGOUT_EVENTS.submitFailed);
    expect(events).not.toContain(HANGOUT_EVENTS.submitted);
  });
});

describe("simulateSubmit", () => {
  it("resolves an event id and approved:false (no network)", async () => {
    const r = await simulateSubmit({ draft: validDraft() });
    expect(r.id).toContain("local-sim-");
    expect(r.approved).toBe(false);
  });
});
