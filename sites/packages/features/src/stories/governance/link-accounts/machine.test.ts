import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  linkAccountsMachine,
  LINK_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveLinkSnapshot,
  slugToState,
  stateToSlug,
  stepsFor,
  type VerifyFn,
  type UnlinkFn,
  type TrackFn,
} from "./machine";
import {
  failClosedVerify,
  failClosedUnlink,
  type VerifyResult,
  type UnlinkResult,
} from "@data/lib/catalyst/governance/link-accounts";

const VERIFIED: VerifyResult = { provider: "forum", verified: true };
const UNLINKED: UnlinkResult = { account: "forum", unlinked: true };

const okVerify: VerifyFn = async ({ provider }) => ({ provider, verified: true });
const failVerify: VerifyFn = async () => {
  throw new Error("signature expired");
};
const okUnlink: UnlinkFn = async ({ account }) => ({ account, unlinked: true });

function inputFor(verify: VerifyFn, track: TrackFn, unlink: UnlinkFn = okUnlink) {
  return {
    account: "forum" as const,
    trackCtx: {
      sid: "sid-abc",
      story: "governance-link-accounts",
      variant: "wizard",
      experimentKey: "gv_link_accounts_wizard",
    },
    verify,
    unlink,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "choosing",
  "connecting",
  "verifying",
  "connected",
  "error",
  "unlinkConfirm",
  "unlinking",
]);

const TRAVERSAL_EVENTS = [
  { type: "CHOOSE" as const, account: "forum" as const },
  { type: "NEXT_STEP" as const },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
  { type: "UNLINK_REQUEST" as const, account: "forum" as const },
  { type: "CONFIRM_UNLINK" as const },
  { type: "CANCEL" as const },
];

describe("linkAccountsMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(linkAccountsMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.choosing);
    expect(slugToState(null)).toBe("choosing");
    expect(slugToState(undefined)).toBe("choosing");
    expect(slugToState("")).toBe("choosing");
    expect(slugToState("nope")).toBe("choosing");
    expect(slugToState("connect")).toBe("connecting");
    expect(slugToState("verifying")).toBe("verifying");
    expect(slugToState("connected")).toBe("connected");
    expect(slugToState("unlink")).toBe("unlinkConfirm");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });

  it("stepsFor: forum/discord are 3-step, push is single subscribe", () => {
    expect(stepsFor("forum")).toBe(3);
    expect(stepsFor("discord")).toBe(3);
    expect(stepsFor("push")).toBe(1);
  });
});

describe("linkAccountsMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveLinkSnapshot({
      step: "choosing",
      account: "forum",
      trackCtx: inputFor(okVerify, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating verifying does NOT fire telemetry and does NOT auto-verify", async () => {
    const track = vi.fn();
    const verify = vi.fn(okVerify);
    const snapshot = resolveLinkSnapshot({
      step: "verifying",
      account: "forum",
      trackCtx: inputFor(verify, track).trackCtx,
      verify,
      track,
    });
    const actor = createActor(linkAccountsMachine, {
      input: inputFor(verify, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("verifying")).toBe(true);
    expect(actor.getSnapshot().context.account).toBe("forum");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(verify).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("verifying")).toBe(true);
  });

  it("hydrating connect seeds the last step so CONFIRM reaches verifying", () => {
    const track = vi.fn();
    const snapshot = resolveLinkSnapshot({
      step: "connecting",
      account: "forum",
      trackCtx: inputFor(okVerify, track).trackCtx,
      track,
    });
    const actor = createActor(linkAccountsMachine, {
      input: inputFor(okVerify, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("connecting")).toBe(true);
    expect(actor.getSnapshot().context.connectStep).toBe(3);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("verifying")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(LINK_EVENTS.verifying);
  });
});

describe("linkAccountsMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(linkAccountsMachine, {
      input: inputFor(okVerify, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("connecting")).toBe(true);
    expect(ends.has("verifying")).toBe(true);
    expect(ends.has("unlinkConfirm")).toBe(true);
  });

  it("reaching verifying passes through CHOOSE and CONFIRM", () => {
    const paths = getShortestPaths(linkAccountsMachine, {
      input: inputFor(okVerify, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const verifying = paths.find((p) => (p.state.value as string) === "verifying");
    expect(verifying).toBeDefined();
    const events = verifying!.steps.map((s) => s.event.type);
    expect(events).toContain("CHOOSE");
    expect(events).toContain("CONFIRM");
  });
});

describe("linkAccountsMachine \u{2014} telemetry events (happy path)", () => {
  it("choose -> step through -> confirm -> verify -> connected fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(linkAccountsMachine, {
      input: inputFor(okVerify, track),
    }).start();

    actor.send({ type: "CHOOSE", account: "forum" });
    expect(actor.getSnapshot().matches("connecting")).toBe(true);

    actor.send({ type: "NEXT_STEP" });
    actor.send({ type: "NEXT_STEP" });
    expect(actor.getSnapshot().context.connectStep).toBe(3);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("connected"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(LINK_EVENTS.started);
    expect(events).toContain(LINK_EVENTS.connectStep);
    expect(events).toContain(LINK_EVENTS.verifying);
    expect(events).toContain(LINK_EVENTS.connected);

    expect(events.indexOf(LINK_EVENTS.verifying)).toBeLessThan(
      events.indexOf(LINK_EVENTS.connected),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === LINK_EVENTS.started);
    expect(startedCall?.[1]).toMatchObject({ account: "forum" });
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_link_accounts_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(VERIFIED);
  });

  it("CONFIRM before the last step is ignored (guard) \u{2014} no premature verify", () => {
    const track = vi.fn();
    const verify = vi.fn(okVerify);
    const actor = createActor(linkAccountsMachine, {
      input: inputFor(verify, track),
    }).start();

    actor.send({ type: "CHOOSE", account: "forum" });
    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("connecting")).toBe(true);
    expect(verify).not.toHaveBeenCalled();
    expect(track.mock.calls.map((c) => c[0])).not.toContain(LINK_EVENTS.verifying);
  });
});

describe("linkAccountsMachine \u{2014} verify failure + retry", () => {
  it("verify error fires gv_link_verify_error; RETRY recovers to connected", async () => {
    const track = vi.fn();
    let calls = 0;
    const verify: VerifyFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("signature expired");
      return okVerify(args);
    };

    const actor = createActor(linkAccountsMachine, {
      input: inputFor(verify, track),
    }).start();

    actor.send({ type: "CHOOSE", account: "discord" });
    actor.send({ type: "NEXT_STEP" });
    actor.send({ type: "NEXT_STEP" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("signature expired");
    expect(track.mock.calls.map((c) => c[0])).toContain(LINK_EVENTS.verifyError);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("connected"));
    expect(track.mock.calls.map((c) => c[0])).toContain(LINK_EVENTS.connected);
  });
});

describe("linkAccountsMachine \u{2014} push (single-step) + unlink", () => {
  it("push confirms immediately (single subscribe step) -> verifying -> connected", async () => {
    const track = vi.fn();
    const actor = createActor(linkAccountsMachine, {
      input: { ...inputFor(okVerify, track), account: "push" },
    }).start();

    actor.send({ type: "CHOOSE", account: "push" });
    expect(actor.getSnapshot().context.totalSteps).toBe(1);
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("connected"));
    expect(track.mock.calls.map((c) => c[0])).toContain(LINK_EVENTS.connected);
  });

  it("UNLINK_REQUEST -> CONFIRM_UNLINK fires gv_link_unlinked and returns to choosing", async () => {
    const track = vi.fn();
    const actor = createActor(linkAccountsMachine, {
      input: inputFor(okVerify, track),
    }).start();

    actor.send({ type: "UNLINK_REQUEST", account: "forum" });
    expect(actor.getSnapshot().matches("unlinkConfirm")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).not.toContain(LINK_EVENTS.unlinked);

    actor.send({ type: "CONFIRM_UNLINK" });
    await waitFor(actor, (s) => s.matches("choosing"));

    expect(track.mock.calls.map((c) => c[0])).toContain(LINK_EVENTS.unlinked);
    expect(actor.getSnapshot().context.unlinkResult).toEqual(UNLINKED);
  });

  it("UNLINK_REQUEST -> CANCEL returns to choosing without unlinking", () => {
    const track = vi.fn();
    const actor = createActor(linkAccountsMachine, {
      input: inputFor(okVerify, track),
    }).start();

    actor.send({ type: "UNLINK_REQUEST", account: "forum" });
    actor.send({ type: "CANCEL" });
    expect(actor.getSnapshot().matches("choosing")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).not.toContain(LINK_EVENTS.unlinked);
  });
});

describe("shipped defaults", () => {
  it("verification fails closed per provider instead of always verifying", async () => {
    await expect(failClosedVerify({ provider: "forum" })).rejects.toThrow(
      /forum challenge service not configured.*DISCOURSE_API_KEY/,
    );
    await expect(failClosedVerify({ provider: "discord" })).rejects.toThrow(
      /Discord verification bot not configured.*DISCORD_TOKEN/,
    );
    await expect(failClosedVerify({ provider: "push" })).rejects.toThrow(
      /Push Protocol subscription is signed by your own wallet/,
    );
  });

  it("unlink fails closed instead of reporting a write that never happened", async () => {
    await expect(failClosedUnlink({ account: "forum" })).rejects.toThrow(
      "account unlink unavailable: governance account service not configured",
    );
  });
});
