import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  subscriptionMachine,
  SUBSCRIPTION_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveSubscriptionSnapshot,
  slugToState,
  stateToSlug,
  enabledTypes,
  simulateCommit,
  type CommitFn,
  type CommitResult,
  type TrackFn,
} from "./machine";

const RESULT: CommitResult = { kind: "subscribe", at: 123 };

const okCommit: CommitFn = async ({ kind }) => ({ kind, at: 1 });
const failCommit: CommitFn = async () => {
  throw new Error("catalyst gone (410)");
};

function inputFor(commit: CommitFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "landings-event-subscriptions",
      variant: "wizard",
      experimentKey: "landings_event_subscriptions",
    },
    selection: { events_started: true, events_starts_soon: false },
    commit,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "idle",
  "signinGate",
  "editing",
  "submitting",
  "subscribed",
  "unsubscribing",
  "unsubscribed",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "START" as const },
  { type: "SIGN_IN" as const },
  { type: "TOGGLE" as const, notificationType: "events_started", enabled: true },
  { type: "SUBMIT" as const },
  { type: "UNSUBSCRIBE" as const },
  { type: "RESUBSCRIBE" as const },
  { type: "EDIT" as const },
  { type: "RETRY" as const },
  { type: "BACK" as const },
];

describe("subscriptionMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(subscriptionMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.idle);
    expect(slugToState(null)).toBe("idle");
    expect(slugToState(undefined)).toBe("idle");
    expect(slugToState("")).toBe("idle");
    expect(slugToState("nope")).toBe("idle");
    expect(slugToState("signin-gate")).toBe("signinGate");
    expect(slugToState("editing")).toBe("editing");
    expect(slugToState("submitting")).toBe("submitting");
    expect(slugToState("unsubscribing")).toBe("unsubscribing");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("subscriptionMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveSubscriptionSnapshot({
      step: "idle",
      trackCtx: inputFor(okCommit, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating signinGate does NOT re-fire its entry telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveSubscriptionSnapshot({
      step: "signinGate",
      trackCtx: inputFor(okCommit, track).trackCtx,
      track,
    });
    const actor = createActor(subscriptionMachine, {
      input: inputFor(okCommit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("signinGate")).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-commit", async () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const snapshot = resolveSubscriptionSnapshot({
      step: "submitting",
      trackCtx: inputFor(commit, track).trackCtx,
      commit,
      track,
    });
    const actor = createActor(subscriptionMachine, {
      input: inputFor(commit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("hydrating unsubscribing seeds lastKind=unsubscribe", () => {
    const snapshot = resolveSubscriptionSnapshot({
      step: "unsubscribing",
      trackCtx: inputFor(okCommit, () => {}).trackCtx,
    });
    const actor = createActor(subscriptionMachine, {
      input: inputFor(okCommit, () => {}),
      snapshot,
    }).start();
    expect(actor.getSnapshot().context.lastKind).toBe("unsubscribe");
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveSubscriptionSnapshot({
      step: "editing",
      trackCtx: inputFor(okCommit, track).trackCtx,
      track,
    });
    const actor = createActor(subscriptionMachine, {
      input: inputFor(okCommit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("editing")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "TOGGLE", notificationType: "events_starts_soon", enabled: true });
    expect(track.mock.calls.map((c) => c[0])).toContain(SUBSCRIPTION_EVENTS.edited);
    expect(actor.getSnapshot().context.selection.events_starts_soon).toBe(true);
  });
});

describe("subscriptionMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(subscriptionMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("signinGate")).toBe(true);
    expect(ends.has("editing")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching submitting passes through START, SIGN_IN, and SUBMIT", () => {
    const paths = getShortestPaths(subscriptionMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("START");
    expect(events).toContain("SIGN_IN");
    expect(events).toContain("SUBMIT");
  });
});

describe("subscriptionMachine \u{2014} telemetry events (happy path)", () => {
  it("start -> gate -> sign in -> submit -> subscribed fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(subscriptionMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("signinGate")).toBe(true);

    actor.send({ type: "SIGN_IN" });
    expect(actor.getSnapshot().matches("editing")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("subscribed"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(SUBSCRIPTION_EVENTS.started);
    expect(events).toContain(SUBSCRIPTION_EVENTS.signinRequired);
    expect(events).toContain(SUBSCRIPTION_EVENTS.signedIn);
    expect(events).toContain(SUBSCRIPTION_EVENTS.submitting);
    expect(events).toContain(SUBSCRIPTION_EVENTS.subscribed);

    expect(events.indexOf(SUBSCRIPTION_EVENTS.started)).toBeLessThan(
      events.indexOf(SUBSCRIPTION_EVENTS.subscribed),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === SUBSCRIPTION_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "landings_event_subscriptions",
      variant: "wizard",
    });
    const subscribedCall = track.mock.calls.find(
      (c) => c[0] === SUBSCRIPTION_EVENTS.subscribed,
    );
    expect(subscribedCall?.[1]).toEqual({ enabled_count: 1 });
    expect(actor.getSnapshot().context.result?.kind).toBe("subscribe");
  });

  it("subscribed -> unsubscribe -> unsubscribed fires the unsubscribe funnel", async () => {
    const track = vi.fn();
    const actor = createActor(subscriptionMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("subscribed"));

    actor.send({ type: "UNSUBSCRIBE" });
    await waitFor(actor, (s) => s.matches("unsubscribed"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(SUBSCRIPTION_EVENTS.unsubscribing);
    expect(events).toContain(SUBSCRIPTION_EVENTS.unsubscribed);
    const unsubscribedCall = track.mock.calls.find(
      (c) => c[0] === SUBSCRIPTION_EVENTS.unsubscribed,
    );
    expect(unsubscribedCall?.[1]).toEqual({});
    expect(actor.getSnapshot().context.lastKind).toBe("unsubscribe");
    expect(actor.getSnapshot().context.result?.kind).toBe("unsubscribe");
  });

  it("TOGGLE updates selection and fires edited with the type+enabled", () => {
    const track = vi.fn();
    const actor = createActor(subscriptionMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "TOGGLE", notificationType: "events_starts_soon", enabled: true });

    expect(actor.getSnapshot().context.selection.events_starts_soon).toBe(true);
    const editedCall = track.mock.calls.find((c) => c[0] === SUBSCRIPTION_EVENTS.edited);
    expect(editedCall?.[1]).toMatchObject({
      notification_type: "events_starts_soon",
      enabled: true,
    });
  });
});

describe("subscriptionMachine \u{2014} error + retry", () => {
  it("commit error -> error state fires landings_subscription_error", async () => {
    const track = vi.fn();
    const actor = createActor(subscriptionMachine, {
      input: inputFor(failCommit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));

    expect(actor.getSnapshot().context.error).toBe("catalyst gone (410)");
    expect(track.mock.calls.map((c) => c[0])).toContain(SUBSCRIPTION_EVENTS.error);
  });

  it("RETRY after a subscribe error re-runs submit and recovers to subscribed", async () => {
    const track = vi.fn();
    let calls = 0;
    const commit: CommitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("catalyst gone (410)");
      return okCommit(args);
    };

    const actor = createActor(subscriptionMachine, {
      input: inputFor(commit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("subscribed"));
    expect(actor.getSnapshot().context.result?.kind).toBe("subscribe");
  });

  it("RETRY after an unsubscribe error re-runs unsubscribe (guarded by lastKind)", async () => {
    const track = vi.fn();
    let calls = 0;
    const commit: CommitFn = async (args) => {
      calls += 1;
      if (args.kind === "unsubscribe" && calls === 2) {
        throw new Error("catalyst gone (410)");
      }
      return okCommit(args);
    };

    const actor = createActor(subscriptionMachine, {
      input: inputFor(commit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("subscribed"));

    actor.send({ type: "UNSUBSCRIBE" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.lastKind).toBe("unsubscribe");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("unsubscribed"));
    expect(actor.getSnapshot().context.result?.kind).toBe("unsubscribe");
  });

  it("BACK from error returns to editing", async () => {
    const actor = createActor(subscriptionMachine, {
      input: inputFor(failCommit, () => {}),
    }).start();

    actor.send({ type: "START" });
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("editing")).toBe(true);
  });
});

describe("simulateCommit + enabledTypes", () => {
  it("simulateCommit resolves a result keyed by kind (no network)", async () => {
    const sub = await simulateCommit({ kind: "subscribe", enabledTypes: [] });
    const unsub = await simulateCommit({ kind: "unsubscribe", enabledTypes: [] });
    expect(sub.kind).toBe("subscribe");
    expect(unsub.kind).toBe("unsubscribe");
    expect(Object.keys(sub).sort()).toEqual(["at", "kind"]);
  });

  it("enabledTypes returns only the on types", () => {
    expect(enabledTypes({ a: true, b: false, c: true })).toEqual(["a", "c"]);
  });

  it("RESULT fixture shape is the CommitResult contract", () => {
    expect(Object.keys(RESULT).sort()).toEqual(["at", "kind"]);
    expect(RESULT.kind).toBe("subscribe");
  });
});
