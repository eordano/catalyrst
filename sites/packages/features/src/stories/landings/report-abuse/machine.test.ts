import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  reportMachine,
  REPORT_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveReportSnapshot,
  slugToState,
  stateToSlug,
  type TrackFn,
} from "./machine";
import {
  failClosedSubmitReport,
  type ReportDraft,
  type SubmitReportFn,
  type SubmitReportResult,
} from "@data/lib/catalyst/landings/report";

const VALID_REPORTED = "0x8ba1f109551bd432803012645ac136ddd64dba72";
const VALID_REPORTER = "0x71c7656ec7ab88b098defb751b7401b5f6d8976f";

const RESULT: SubmitReportResult = {
  reportId: "report-test",
  evidenceKeys: ["evidence/0/clip.mp4"],
};

const okSubmit: SubmitReportFn = async () => RESULT;

function inputFor(submit: SubmitReportFn, track: TrackFn, playerAddress = VALID_REPORTER) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "landings-report-abuse",
      variant: "wizard",
      experimentKey: "landings_report_wizard",
    },
    playerAddress,
    submit,
    track,
  };
}

function driveToReview(actor: ReturnType<typeof createActor>) {
  actor.send({ type: "START" });
  actor.send({ type: "SET_TARGET", reportedAddress: VALID_REPORTED });
  actor.send({ type: "SET_CATEGORY", reason: "harassment" });
  actor.send({ type: "SET_DETAILS", description: "They harassed me in chat." });
  actor.send({
    type: "SET_EVIDENCE",
    evidence: [{ id: "e1", name: "shot.png", size: 100 }],
  });
  actor.send({ type: "CONTINUE" });
  actor.send({ type: "SET_CONFIRM", confirmAccuracy: true });
}

const EXPECTED_STATES = new Set([
  "intro",
  "target",
  "category",
  "details",
  "evidence",
  "review",
  "submitting",
  "success",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "START" as const },
  { type: "SET_TARGET" as const, reportedAddress: VALID_REPORTED },
  { type: "SET_TARGET" as const, reportedAddress: "not-an-address" },
  { type: "SET_CATEGORY" as const, reason: "harassment" as const },
  { type: "SET_DETAILS" as const, description: "They harassed me." },
  { type: "SET_DETAILS" as const, description: "   " },
  { type: "SET_EVIDENCE" as const, evidence: [{ id: "e1", name: "a.png", size: 1 }] },
  { type: "CONTINUE" as const },
  { type: "SET_CONFIRM" as const, confirmAccuracy: true },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

function failedSteps(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls
    .filter((c) => c[0] === REPORT_EVENTS.validationFailed)
    .map((c) => (c[1] as { step?: string } | undefined)?.step);
}

describe("reportMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping slug and falls back to intro", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(reportMachine.states)));
    expect(mapped).toEqual(EXPECTED_STATES);
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.intro);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("intro");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("reportMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots intro without a snapshot, hydrates submitting silently, and only real transitions track", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const trackCtx = inputFor(submit, track).trackCtx;
    expect(resolveReportSnapshot({ step: "intro", trackCtx })).toBeUndefined();

    const seed: Partial<ReportDraft> = {
      reportedAddress: VALID_REPORTED,
      reason: "harassment",
      description: "x",
      evidence: [{ id: "e", name: "f.png", size: 1 }],
      confirmAccuracy: true,
    };
    const submitting = createActor(reportMachine, {
      input: inputFor(submit, track),
      snapshot: resolveReportSnapshot({ step: "submitting", trackCtx, seed, submit, track }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.draft.reportedAddress).toBe(VALID_REPORTED);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const category = createActor(reportMachine, {
      input: inputFor(okSubmit, track),
      snapshot: resolveReportSnapshot({
        step: "category",
        trackCtx,
        seed: { reportedAddress: VALID_REPORTED },
        track,
      }),
    }).start();
    expect(category.getSnapshot().matches("category")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    category.send({ type: "SET_CATEGORY", reason: "impersonation" });
    expect(category.getSnapshot().matches("details")).toBe(true);
    const categoryCall = track.mock.calls.find((c) => c[0] === REPORT_EVENTS.categorySet);
    expect(categoryCall?.[1]).toMatchObject({ reason: "impersonation" });
  });
});

describe("reportMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and submitting needs the full funnel + confirm", () => {
    const paths = getShortestPaths(reportMachine, {
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
    for (const s of ["target", "category", "details", "evidence", "review", "submitting"]) {
      expect(ends.has(s)).toBe(true);
    }
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    const events = submitting!.steps.map((s) => s.event.type);
    for (const e of [
      "START",
      "SET_TARGET",
      "SET_CATEGORY",
      "SET_DETAILS",
      "SET_EVIDENCE",
      "CONTINUE",
      "SET_CONFIRM",
      "SUBMIT",
    ]) {
      expect(events).toContain(e);
    }
  });
});

describe("reportMachine \u{2014} telemetry events (happy path)", () => {
  it("intro -> ... -> submit -> success fires the full funnel in order", async () => {
    const track = vi.fn();
    const actor = createActor(reportMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    driveToReview(actor);
    expect(actor.getSnapshot().matches("review")).toBe(true);
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = names(track);
    for (const e of [
      REPORT_EVENTS.started,
      REPORT_EVENTS.targetSet,
      REPORT_EVENTS.categorySet,
      REPORT_EVENTS.detailsSet,
      REPORT_EVENTS.evidenceAdded,
      REPORT_EVENTS.reviewReached,
      REPORT_EVENTS.submitStarted,
      REPORT_EVENTS.completed,
    ]) {
      expect(events).toContain(e);
    }
    expect(events.indexOf(REPORT_EVENTS.submitStarted)).toBeLessThan(
      events.indexOf(REPORT_EVENTS.completed),
    );
    const startedCall = track.mock.calls.find((c) => c[0] === REPORT_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "landings_report_wizard",
      variant: "wizard",
    });
    const completedCall = track.mock.calls.find((c) => c[0] === REPORT_EVENTS.completed);
    expect(completedCall?.[1]).toMatchObject({ report_id: RESULT.reportId, evidence_count: 1 });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("reportMachine \u{2014} validation guardrails (self-loops)", () => {
  it("each step self-loops on invalid input and fires report_validation_failed with the step", () => {
    const track = vi.fn();
    const actor = createActor(reportMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send({ type: "SET_TARGET", reportedAddress: "nope" });
    expect(actor.getSnapshot().matches("target")).toBe(true);
    const failed = track.mock.calls.filter((c) => c[0] === REPORT_EVENTS.validationFailed);
    expect(failed.length).toBe(1);
    expect(failed[0][1]).toMatchObject({ step: "target", fields: ["reportedAddress"] });

    actor.send({ type: "SET_TARGET", reportedAddress: VALID_REPORTED });
    actor.send({ type: "SET_CATEGORY", reason: "cheating" });
    actor.send({ type: "SET_DETAILS", description: "   " });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(failedSteps(track)).toContain("details");

    actor.send({ type: "SET_DETAILS", description: "happened in plaza" });
    actor.send({ type: "CONTINUE" });
    expect(actor.getSnapshot().matches("evidence")).toBe(true);
    expect(failedSteps(track)).toContain("evidence");

    actor.send({
      type: "SET_EVIDENCE",
      evidence: [
        { id: "a", name: "a.png", size: 1 },
        { id: "b", name: "b.mp4", size: 2 },
      ],
    });
    expect(actor.getSnapshot().matches("evidence")).toBe(true);
    const added = track.mock.calls.find((c) => c[0] === REPORT_EVENTS.evidenceAdded);
    expect(added?.[1]).toMatchObject({ file_count: 2 });

    actor.send({ type: "CONTINUE" });
    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(failedSteps(track)).toContain("review");
  });
});

describe("reportMachine \u{2014} submit failure + retry", () => {
  it("submit error -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submit: SubmitReportFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("report ingest unreachable");
      return okSubmit(args);
    };
    const actor = createActor(reportMachine, {
      input: inputFor(submit, track),
    }).start();

    driveToReview(actor);
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("report ingest unreachable");
    expect(names(track).filter((e) => e === REPORT_EVENTS.failed)).toHaveLength(1);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(names(track)).toContain(REPORT_EVENTS.completed);
  });
});

describe("failClosedSubmitReport", () => {
  it("fails closed instead of fabricating a report id and evidence keys", async () => {
    const draft: ReportDraft = {
      playerAddress: VALID_REPORTER,
      reportedAddress: VALID_REPORTED,
      reason: "harassment",
      description: "x",
      evidence: [{ id: "e", name: "clip.mp4", size: 10 }],
      additionalComments: "",
      confirmAccuracy: true,
    };
    await expect(failClosedSubmitReport({ draft })).rejects.toThrow(
      "report submission unavailable: report service not configured",
    );
  });
});
