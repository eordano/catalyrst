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
  it("covers every state, round-trips uniquely, falls back to the first step; forum/discord are 3-step and push is a single subscribe", () => {
    const machineStates = new Set(Object.keys(linkAccountsMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.choosing);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("choosing");
    }
    expect(slugToState("connect")).toBe("connecting");
    expect(slugToState("verifying")).toBe("verifying");
    expect(slugToState("connected")).toBe("connected");
    expect(slugToState("unlink")).toBe("unlinkConfirm");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);

    expect(stepsFor("forum")).toBe(3);
    expect(stepsFor("discord")).toBe(3);
    expect(stepsFor("push")).toBe(1);
  });
});

describe("linkAccountsMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot; verifying hydrates without telemetry or auto-verify; connect seeds the last step so CONFIRM reaches verifying", async () => {
    const trackCtx = inputFor(okVerify, () => {}).trackCtx;
    expect(resolveLinkSnapshot({ step: "choosing", account: "forum", trackCtx })).toBeUndefined();

    const track = vi.fn();
    const verify = vi.fn(okVerify);
    const verifying = createActor(linkAccountsMachine, {
      input: inputFor(verify, track),
      snapshot: resolveLinkSnapshot({ step: "verifying", account: "forum", trackCtx, verify, track }),
    }).start();
    expect(verifying.getSnapshot().matches("verifying")).toBe(true);
    expect(verifying.getSnapshot().context.account).toBe("forum");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(verify).not.toHaveBeenCalled();
    expect(verifying.getSnapshot().matches("verifying")).toBe(true);

    const connecting = createActor(linkAccountsMachine, {
      input: inputFor(okVerify, track),
      snapshot: resolveLinkSnapshot({ step: "connecting", account: "forum", trackCtx, track }),
    }).start();
    expect(connecting.getSnapshot().matches("connecting")).toBe(true);
    expect(connecting.getSnapshot().context.connectStep).toBe(3);
    expect(track).not.toHaveBeenCalled();

    connecting.send({ type: "CONFIRM" });
    expect(connecting.getSnapshot().matches("verifying")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(LINK_EVENTS.verifying);
  });
});

describe("linkAccountsMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and verifying passes through CHOOSE and CONFIRM", () => {
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
    for (const s of ["connecting", "verifying", "unlinkConfirm"]) {
      expect(ends.has(s)).toBe(true);
    }

    const verifying = paths.find((p) => (p.state.value as string) === "verifying");
    expect(verifying).toBeDefined();
    expect(verifying!.steps.map((s) => s.event.type)).toEqual(
      expect.arrayContaining(["CHOOSE", "CONFIRM"]),
    );
  });
});

describe("linkAccountsMachine \u{2014} telemetry events (happy path)", () => {
  it("CONFIRM before the last step is ignored; choose -> step through -> confirm -> verify -> connected fires the full funnel", async () => {
    const track = vi.fn();
    const verify = vi.fn(okVerify);
    const actor = createActor(linkAccountsMachine, {
      input: inputFor(verify, track),
    }).start();

    actor.send({ type: "CHOOSE", account: "forum" });
    expect(actor.getSnapshot().matches("connecting")).toBe(true);

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("connecting")).toBe(true);
    expect(verify).not.toHaveBeenCalled();
    expect(track.mock.calls.map((c) => c[0])).not.toContain(LINK_EVENTS.verifying);

    actor.send({ type: "NEXT_STEP" });
    actor.send({ type: "NEXT_STEP" });
    expect(actor.getSnapshot().context.connectStep).toBe(3);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("connected"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        LINK_EVENTS.started,
        LINK_EVENTS.connectStep,
        LINK_EVENTS.verifying,
        LINK_EVENTS.connected,
      ]),
    );
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

  it("UNLINK_REQUEST -> CANCEL returns to choosing without unlinking; UNLINK_REQUEST -> CONFIRM_UNLINK fires gv_link_unlinked and returns to choosing", async () => {
    const track = vi.fn();
    const actor = createActor(linkAccountsMachine, {
      input: inputFor(okVerify, track),
    }).start();

    actor.send({ type: "UNLINK_REQUEST", account: "forum" });
    expect(actor.getSnapshot().matches("unlinkConfirm")).toBe(true);
    actor.send({ type: "CANCEL" });
    expect(actor.getSnapshot().matches("choosing")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).not.toContain(LINK_EVENTS.unlinked);

    actor.send({ type: "UNLINK_REQUEST", account: "forum" });
    expect(actor.getSnapshot().matches("unlinkConfirm")).toBe(true);
    actor.send({ type: "CONFIRM_UNLINK" });
    await waitFor(actor, (s) => s.matches("choosing"));

    expect(track.mock.calls.map((c) => c[0])).toContain(LINK_EVENTS.unlinked);
    expect(actor.getSnapshot().context.unlinkResult).toEqual(UNLINKED);
  });
});

describe("shipped defaults", () => {
  it("verification fails closed per provider and unlink fails closed instead of reporting a write that never happened", async () => {
    await expect(failClosedVerify({ provider: "forum" })).rejects.toThrow(
      /forum challenge service not configured.*DISCOURSE_API_KEY/,
    );
    await expect(failClosedVerify({ provider: "discord" })).rejects.toThrow(
      /Discord verification bot not configured.*DISCORD_TOKEN/,
    );
    await expect(failClosedVerify({ provider: "push" })).rejects.toThrow(
      /Push Protocol subscription is signed by your own wallet/,
    );
    await expect(failClosedUnlink({ account: "forum" })).rejects.toThrow(
      "account unlink unavailable: governance account service not configured",
    );
  });
});
