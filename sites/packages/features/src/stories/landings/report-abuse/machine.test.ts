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
const failSubmit: SubmitReportFn = async () => {
  throw new Error("report ingest unreachable");
};

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

describe("reportMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(reportMachine.states));
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

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.intro);
    expect(slugToState(null)).toBe("intro");
    expect(slugToState(undefined)).toBe("intro");
    expect(slugToState("")).toBe("intro");
    expect(slugToState("nope")).toBe("intro");
    expect(slugToState("evidence")).toBe("evidence");
    expect(slugToState("review")).toBe("review");
    expect(slugToState("submitting")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("reportMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveReportSnapshot({
      step: "intro",
      trackCtx: inputFor(okSubmit, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const seed: Partial<ReportDraft> = {
      reportedAddress: VALID_REPORTED,
      reason: "harassment",
      description: "x",
      evidence: [{ id: "e", name: "f.png", size: 1 }],
      confirmAccuracy: true,
    };
    const snapshot = resolveReportSnapshot({
      step: "submitting",
      trackCtx: inputFor(submit, track).trackCtx,
      seed,
      submit,
      track,
    });
    const actor = createActor(reportMachine, {
      input: inputFor(submit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.draft.reportedAddress).toBe(VALID_REPORTED);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveReportSnapshot({
      step: "category",
      trackCtx: inputFor(okSubmit, track).trackCtx,
      seed: { reportedAddress: VALID_REPORTED },
      track,
    });
    const actor = createActor(reportMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("category")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SET_CATEGORY", reason: "cheating" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(REPORT_EVENTS.categorySet);
  });
});

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

describe("reportMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
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
  });

  it("reaching submitting passes through the full funnel + confirm", () => {
    const paths = getShortestPaths(reportMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("START");
    expect(events).toContain("SET_TARGET");
    expect(events).toContain("SET_CATEGORY");
    expect(events).toContain("SET_DETAILS");
    expect(events).toContain("SET_EVIDENCE");
    expect(events).toContain("CONTINUE");
    expect(events).toContain("SET_CONFIRM");
    expect(events).toContain("SUBMIT");
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

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(REPORT_EVENTS.started);
    expect(events).toContain(REPORT_EVENTS.targetSet);
    expect(events).toContain(REPORT_EVENTS.categorySet);
    expect(events).toContain(REPORT_EVENTS.detailsSet);
    expect(events).toContain(REPORT_EVENTS.evidenceAdded);
    expect(events).toContain(REPORT_EVENTS.reviewReached);
    expect(events).toContain(REPORT_EVENTS.submitStarted);
    expect(events).toContain(REPORT_EVENTS.completed);

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

  it("category_set carries the chosen reason", () => {
    const track = vi.fn();
    const actor = createActor(reportMachine, {
      input: inputFor(okSubmit, track),
    }).start();
    actor.send({ type: "START" });
    actor.send({ type: "SET_TARGET", reportedAddress: VALID_REPORTED });
    actor.send({ type: "SET_CATEGORY", reason: "impersonation" });

    const call = track.mock.calls.find((c) => c[0] === REPORT_EVENTS.categorySet);
    expect(call?.[1]).toMatchObject({ reason: "impersonation" });
  });
});

describe("reportMachine \u{2014} validation guardrails (self-loops)", () => {
  it("an invalid target stays on target and fires report_validation_failed", () => {
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
  });

  it("empty details stays on details and fires report_validation_failed", () => {
    const track = vi.fn();
    const actor = createActor(reportMachine, {
      input: inputFor(okSubmit, track),
    }).start();
    actor.send({ type: "START" });
    actor.send({ type: "SET_TARGET", reportedAddress: VALID_REPORTED });
    actor.send({ type: "SET_CATEGORY", reason: "cheating" });
    actor.send({ type: "SET_DETAILS", description: "   " });

    expect(actor.getSnapshot().matches("details")).toBe(true);
    const failed = track.mock.calls.filter((c) => c[0] === REPORT_EVENTS.validationFailed);
    expect(failed.some((c) => c[1] && (c[1] as { step?: string }).step === "details")).toBe(true);
  });

  it("CONTINUE with no evidence stays on evidence and fires the guardrail", () => {
    const track = vi.fn();
    const actor = createActor(reportMachine, {
      input: inputFor(okSubmit, track),
    }).start();
    actor.send({ type: "START" });
    actor.send({ type: "SET_TARGET", reportedAddress: VALID_REPORTED });
    actor.send({ type: "SET_CATEGORY", reason: "cheating" });
    actor.send({ type: "SET_DETAILS", description: "happened in plaza" });
    actor.send({ type: "CONTINUE" });

    expect(actor.getSnapshot().matches("evidence")).toBe(true);
    const failed = track.mock.calls.filter((c) => c[0] === REPORT_EVENTS.validationFailed);
    expect(failed.some((c) => c[1] && (c[1] as { step?: string }).step === "evidence")).toBe(true);
  });

  it("SUBMIT without confirming stays on review and fires the guardrail", () => {
    const track = vi.fn();
    const actor = createActor(reportMachine, {
      input: inputFor(okSubmit, track),
    }).start();
    actor.send({ type: "START" });
    actor.send({ type: "SET_TARGET", reportedAddress: VALID_REPORTED });
    actor.send({ type: "SET_CATEGORY", reason: "cheating" });
    actor.send({ type: "SET_DETAILS", description: "happened in plaza" });
    actor.send({ type: "SET_EVIDENCE", evidence: [{ id: "e", name: "f.png", size: 1 }] });
    actor.send({ type: "CONTINUE" });
    actor.send({ type: "SUBMIT" });

    expect(actor.getSnapshot().matches("review")).toBe(true);
    const failed = track.mock.calls.filter((c) => c[0] === REPORT_EVENTS.validationFailed);
    expect(failed.some((c) => c[1] && (c[1] as { step?: string }).step === "review")).toBe(true);
  });

  it("evidence add records the running file count", () => {
    const track = vi.fn();
    const actor = createActor(reportMachine, {
      input: inputFor(okSubmit, track),
    }).start();
    actor.send({ type: "START" });
    actor.send({ type: "SET_TARGET", reportedAddress: VALID_REPORTED });
    actor.send({ type: "SET_CATEGORY", reason: "cheating" });
    actor.send({ type: "SET_DETAILS", description: "happened in plaza" });
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

    const failed = track.mock.calls.filter((c) => c[0] === REPORT_EVENTS.failed);
    expect(failed.length).toBe(1);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(track.mock.calls.map((c) => c[0])).toContain(REPORT_EVENTS.completed);
  });

  it("always fails -> stays in error", async () => {
    const track = vi.fn();
    const actor = createActor(reportMachine, {
      input: inputFor(failSubmit, track),
    }).start();
    driveToReview(actor);
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().matches("error")).toBe(true);
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
