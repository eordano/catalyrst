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
  type TrackFn,
} from "./machine";

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

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("subscriptionMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping slug and falls back to idle", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(subscriptionMachine.states)));
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(slugToState("signin-gate")).toBe("signinGate");
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.idle);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("idle");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("subscriptionMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots idle without a snapshot, hydrates later steps silently, and only real transitions track", async () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const trackCtx = inputFor(commit, track).trackCtx;
    expect(resolveSubscriptionSnapshot({ step: "idle", trackCtx })).toBeUndefined();

    const gate = createActor(subscriptionMachine, {
      input: inputFor(okCommit, track),
      snapshot: resolveSubscriptionSnapshot({ step: "signinGate", trackCtx, track }),
    }).start();
    expect(gate.getSnapshot().matches("signinGate")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    const submitting = createActor(subscriptionMachine, {
      input: inputFor(commit, track),
      snapshot: resolveSubscriptionSnapshot({ step: "submitting", trackCtx, commit, track }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const unsubscribing = createActor(subscriptionMachine, {
      input: inputFor(okCommit, () => {}),
      snapshot: resolveSubscriptionSnapshot({ step: "unsubscribing", trackCtx }),
    }).start();
    expect(unsubscribing.getSnapshot().context.lastKind).toBe("unsubscribe");

    const editing = createActor(subscriptionMachine, {
      input: inputFor(okCommit, track),
      snapshot: resolveSubscriptionSnapshot({ step: "editing", trackCtx, track }),
    }).start();
    expect(editing.getSnapshot().matches("editing")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    editing.send({ type: "TOGGLE", notificationType: "events_starts_soon", enabled: true });
    expect(editing.getSnapshot().context.selection.events_starts_soon).toBe(true);
    const editedCall = track.mock.calls.find((c) => c[0] === SUBSCRIPTION_EVENTS.edited);
    expect(editedCall?.[1]).toMatchObject({
      notification_type: "events_starts_soon",
      enabled: true,
    });
  });
});

describe("subscriptionMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("event paths reach signinGate, editing and submitting, and submitting needs START, SIGN_IN, SUBMIT", () => {
    const paths = getShortestPaths(subscriptionMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
    }
    for (const s of ["signinGate", "editing", "submitting"]) expect(ends.has(s)).toBe(true);
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    const events = submitting!.steps.map((s) => s.event.type);
    for (const e of ["START", "SIGN_IN", "SUBMIT"]) expect(events).toContain(e);
  });
});

describe("subscriptionMachine \u{2014} telemetry events (happy path)", () => {
  it("start -> gate -> sign in -> submit -> subscribed -> unsubscribe fires both funnels", async () => {
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

    let events = names(track);
    for (const e of [
      SUBSCRIPTION_EVENTS.started,
      SUBSCRIPTION_EVENTS.signinRequired,
      SUBSCRIPTION_EVENTS.signedIn,
      SUBSCRIPTION_EVENTS.submitting,
      SUBSCRIPTION_EVENTS.subscribed,
    ]) {
      expect(events).toContain(e);
    }
    expect(events.indexOf(SUBSCRIPTION_EVENTS.started)).toBeLessThan(
      events.indexOf(SUBSCRIPTION_EVENTS.subscribed),
    );
    const startedCall = track.mock.calls.find((c) => c[0] === SUBSCRIPTION_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "landings_event_subscriptions",
      variant: "wizard",
    });
    const subscribedCall = track.mock.calls.find((c) => c[0] === SUBSCRIPTION_EVENTS.subscribed);
    expect(subscribedCall?.[1]).toEqual({ enabled_count: 1 });
    expect(actor.getSnapshot().context.result?.kind).toBe("subscribe");

    actor.send({ type: "UNSUBSCRIBE" });
    await waitFor(actor, (s) => s.matches("unsubscribed"));
    events = names(track);
    expect(events).toContain(SUBSCRIPTION_EVENTS.unsubscribing);
    expect(events).toContain(SUBSCRIPTION_EVENTS.unsubscribed);
    const unsubscribedCall = track.mock.calls.find(
      (c) => c[0] === SUBSCRIPTION_EVENTS.unsubscribed,
    );
    expect(unsubscribedCall?.[1]).toEqual({});
    expect(actor.getSnapshot().context.lastKind).toBe("unsubscribe");
    expect(actor.getSnapshot().context.result?.kind).toBe("unsubscribe");
  });
});

describe("subscriptionMachine \u{2014} error + retry", () => {
  it("commit error fires landings_subscription_error and BACK returns to editing", async () => {
    const track = vi.fn();
    const actor = createActor(subscriptionMachine, {
      input: inputFor(failCommit, track),
    }).start();
    actor.send({ type: "START" });
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("catalyst gone (410)");
    expect(names(track)).toContain(SUBSCRIPTION_EVENTS.error);

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("editing")).toBe(true);
  });

  it("RETRY re-runs the last kind: subscribe after a subscribe error, unsubscribe after an unsubscribe error", async () => {
    let calls = 0;
    const commit: CommitFn = async (args) => {
      calls += 1;
      if (calls === 1 || (args.kind === "unsubscribe" && calls === 3)) {
        throw new Error("catalyst gone (410)");
      }
      return okCommit(args);
    };
    const actor = createActor(subscriptionMachine, {
      input: inputFor(commit, vi.fn()),
    }).start();

    actor.send({ type: "START" });
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));
    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("subscribed"));
    expect(actor.getSnapshot().context.result?.kind).toBe("subscribe");

    actor.send({ type: "UNSUBSCRIBE" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.lastKind).toBe("unsubscribe");
    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("unsubscribed"));
    expect(actor.getSnapshot().context.result?.kind).toBe("unsubscribe");
    expect(calls).toBe(4);
  });
});

describe("simulateCommit + enabledTypes", () => {
  it("simulateCommit resolves a CommitResult keyed by kind and enabledTypes returns only the on types", async () => {
    const sub = await simulateCommit({ kind: "subscribe", enabledTypes: [] });
    const unsub = await simulateCommit({ kind: "unsubscribe", enabledTypes: [] });
    expect(sub.kind).toBe("subscribe");
    expect(unsub.kind).toBe("unsubscribe");
    expect(enabledTypes({ a: true, b: false, c: true })).toEqual(["a", "c"]);
  });
});
