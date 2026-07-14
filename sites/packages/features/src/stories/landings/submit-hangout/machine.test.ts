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

describe("hangoutMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(hangoutMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);
  });

  it("slugs are unique and round-trip via SLUG_TO_STATE", () => {
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
    }
  });

  it("slugs equal the audit-spec step ids", () => {
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
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.signinGate);
    expect(slugToState(null)).toBe("signinGate");
    expect(slugToState(undefined)).toBe("signinGate");
    expect(slugToState("")).toBe("signinGate");
    expect(slugToState("nope")).toBe("signinGate");
    expect(slugToState("schedule")).toBe("schedule");
    expect(slugToState("submitting")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("hangoutMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveHangoutSnapshot({
      step: "signinGate",
      trackCtx: inputFor(okSubmit, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const snapshot = resolveHangoutSnapshot({
      step: "submitting",
      trackCtx: inputFor(submit, track).trackCtx,
      submit,
      track,
    });
    const actor = createActor(hangoutMachine, {
      input: inputFor(submit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveHangoutSnapshot({
      step: "review",
      trackCtx: inputFor(okSubmit, track).trackCtx,
      track,
    });
    const actor = createActor(hangoutMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "PREVIEW" });
    expect(actor.getSnapshot().matches("preview")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(HANGOUT_EVENTS.previewOpened);
  });

  it("hydrating signinGate fires the gate-viewed event (entry)", () => {
    const track = vi.fn();
    const actor = createActor(hangoutMachine, {
      input: inputFor(okSubmit, track),
    }).start();
    expect(actor.getSnapshot().matches("signinGate")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(HANGOUT_EVENTS.gateViewed);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "SIGN_IN" as const },
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "PREVIEW" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

describe("hangoutMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
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
  });

  it("reaching review passes through SIGN_IN and the form steps", () => {
    const paths = getShortestPaths(hangoutMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toContain("SIGN_IN");
    expect(events.filter((e) => e === "NEXT").length).toBeGreaterThanOrEqual(4);
  });
});

describe("hangoutMachine \u{2014} per-step validation guards", () => {
  it("details with an empty name does NOT advance", () => {
    const actor = createActor(hangoutMachine, {
      input: inputFor(okSubmit, () => {}, { ...emptyDraft() }),
    }).start();
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("details")).toBe(true);

    actor.send({ type: "EDIT", patch: { name: "My Hangout" } });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("location")).toBe(true);
  });

  it("schedule without a date does NOT advance to review", () => {
    const draft = { ...validDraft(), startDate: "", startTime: "" };
    const snapshot = resolveHangoutSnapshot({
      step: "schedule",
      trackCtx: inputFor(okSubmit, () => {}).trackCtx,
      draft,
    });
    const actor = createActor(hangoutMachine, {
      input: inputFor(okSubmit, () => {}, draft),
      snapshot,
    }).start();
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("schedule")).toBe(true);

    actor.send({ type: "EDIT", patch: { startDate: "2026-07-18", startTime: "20:00" } });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
  });
});

describe("hangoutMachine \u{2014} telemetry (happy path)", () => {
  it("gate -> form steps -> submit -> submitted fires the funnel", async () => {
    const track = vi.fn();
    const actor = createActor(hangoutMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitted"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(HANGOUT_EVENTS.gateViewed);
    expect(events).toContain(HANGOUT_EVENTS.started);
    expect(events).toContain(HANGOUT_EVENTS.submitAttempted);
    expect(events).toContain(HANGOUT_EVENTS.submitted);

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

  it("preview opens before submit and fires preview_opened", () => {
    const track = vi.fn();
    const snapshot = resolveHangoutSnapshot({
      step: "review",
      trackCtx: inputFor(okSubmit, track).trackCtx,
      track,
    });
    const actor = createActor(hangoutMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();
    actor.send({ type: "PREVIEW" });
    expect(actor.getSnapshot().matches("preview")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(HANGOUT_EVENTS.previewOpened);
  });
});

describe("hangoutMachine \u{2014} submit failure", () => {
  it("submit error returns to review and fires submit_failed", async () => {
    const track = vi.fn();
    const snapshot = resolveHangoutSnapshot({
      step: "review",
      trackCtx: inputFor(failSubmit, track).trackCtx,
      submit: failSubmit,
      track,
    });
    const actor = createActor(hangoutMachine, {
      input: inputFor(failSubmit, track),
      snapshot,
    }).start();

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("review"));
    expect(actor.getSnapshot().context.error).toBe("create endpoint is admin-gated");

    const events = track.mock.calls.map((c) => c[0]);
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
