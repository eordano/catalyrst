import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  castMachine,
  CAST_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  DEFAULT_DEVICES,
  resolveCastSnapshot,
  slugToState,
  stateToSlug,
  simulateResolveToken,
  simulateGrant,
  simulateShareScreen,
  type ResolveTokenFn,
  type RequestPermissionsFn,
  type EndCastFn,
  type ShareScreenFn,
  type TokenResult,
  type TrackFn,
} from "./machine";

const RESULT: TokenResult = {
  info: { placeName: "Test Plaza", placeId: "p1", location: "1,2", isWorld: false },
  credentials: {
    url: "wss://livekit.test",
    token: "SIMULATED.test.stub",
    roomId: "scene-1:2-test-plaza",
    identity: "Speaker",
  },
};

const okResolve: ResolveTokenFn = async () => RESULT;
const badResolve: ResolveTokenFn = async () => {
  throw new Error("Streaming token has expired");
};
const grant: RequestPermissionsFn = async () => ({ granted: true });
const deny: RequestPermissionsFn = async () => ({ granted: false });
const endOk: EndCastFn = async () => {};

function inputFor(
  resolveToken: ResolveTokenFn,
  requestPermissions: RequestPermissionsFn,
  track: TrackFn,
) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "landings-cast-stream",
      variant: "console",
      experimentKey: "st_cast_console",
    },
    token: "stream-key-123",
    identity: "Speaker",
    resolveToken,
    requestPermissions,
    endCast: endOk,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "tokenCheck",
  "deviceSelect",
  "permissions",
  "preview",
  "live",
  "ending",
  "ended",
  "invalid",
]);

describe("castMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(castMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);
  });

  it("slugs are the audit-spec step names, unique, and round-trip", () => {
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    expect(slugs).toEqual([
      "token-check",
      "device-select",
      "permissions",
      "preview",
      "live",
      "ending",
      "ended",
      "invalid",
    ]);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
    }
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.tokenCheck);
    expect(slugToState(null)).toBe("tokenCheck");
    expect(slugToState(undefined)).toBe("tokenCheck");
    expect(slugToState("")).toBe("tokenCheck");
    expect(slugToState("nope")).toBe("tokenCheck");
    expect(slugToState("device-select")).toBe("deviceSelect");
    expect(slugToState("permissions")).toBe("permissions");
    expect(slugToState("live")).toBe("live");
    expect(slugToState("invalid")).toBe("invalid");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("castMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveCastSnapshot({
      step: "tokenCheck",
      trackCtx: inputFor(okResolve, grant, () => {}).trackCtx,
      token: "k",
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-run actors", async () => {
    const track = vi.fn();
    const resolveToken = vi.fn(okResolve);
    const requestPermissions = vi.fn(grant);
    const snapshot = resolveCastSnapshot({
      step: "preview",
      trackCtx: inputFor(resolveToken, requestPermissions, track).trackCtx,
      token: "k",
      resolveToken,
      requestPermissions,
      track,
    });
    const actor = createActor(castMachine, {
      input: inputFor(resolveToken, requestPermissions, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("preview")).toBe(true);
    expect(actor.getSnapshot().context.info?.placeName).toBe("Genesis Plaza");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(resolveToken).not.toHaveBeenCalled();
    expect(requestPermissions).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("preview")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveCastSnapshot({
      step: "live",
      trackCtx: inputFor(okResolve, grant, track).trackCtx,
      token: "k",
      track,
    });
    const actor = createActor(castMachine, {
      input: inputFor(okResolve, grant, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("live")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "LEAVE" });
    expect(track.mock.calls.map((c) => c[0])).toContain(CAST_EVENTS.ending);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "SELECT_DEVICES" as const, devices: DEFAULT_DEVICES },
  { type: "GRANT" as const },
  { type: "RETRY_PERMISSIONS" as const },
  { type: "JOIN" as const },
  { type: "BACK" as const },
  { type: "TOGGLE_SCREENSHARE" as const },
  { type: "LEAVE" as const },
  { type: "RETRY" as const },
];

function topLevel(value: unknown): string {
  return typeof value === "string" ? value : Object.keys(value as object)[0];
}

describe("castMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected (top-level) state", () => {
    const paths = getShortestPaths(castMachine, {
      input: inputFor(okResolve, grant, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    for (const p of paths) {
      expect(EXPECTED_STATES.has(topLevel(p.state.value))).toBe(true);
    }
  });
});

describe("castMachine \u{2014} happy path (token -> devices -> permissions -> preview -> live)", () => {
  it("fires the full funnel and reaches live", async () => {
    const track = vi.fn();
    const actor = createActor(castMachine, {
      input: inputFor(okResolve, grant, track),
    }).start();

    await waitFor(actor, (s) => s.matches("deviceSelect"));

    actor.send({ type: "SELECT_DEVICES", devices: DEFAULT_DEVICES });
    await waitFor(actor, (s) => s.matches("preview"));

    actor.send({ type: "JOIN" });
    expect(actor.getSnapshot().matches("live")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CAST_EVENTS.tokenChecked);
    expect(events).toContain(CAST_EVENTS.tokenValid);
    expect(events).toContain(CAST_EVENTS.devicesSelected);
    expect(events).toContain(CAST_EVENTS.permissionsGranted);
    expect(events).toContain(CAST_EVENTS.previewReady);
    expect(events).toContain(CAST_EVENTS.joinRequested);
    expect(events).toContain(CAST_EVENTS.wentLive);

    expect(events.indexOf(CAST_EVENTS.tokenChecked)).toBeLessThan(
      events.indexOf(CAST_EVENTS.wentLive),
    );

    const liveCall = track.mock.calls.find((c) => c[0] === CAST_EVENTS.wentLive);
    expect(liveCall?.[1]).toMatchObject({ stub: true });
    expect(liveCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "st_cast_console",
      variant: "console",
    });
  });

  it("live -> LEAVE -> ending -> ended fires teardown telemetry", async () => {
    const track = vi.fn();
    const actor = createActor(castMachine, {
      input: inputFor(okResolve, grant, track),
    }).start();
    await waitFor(actor, (s) => s.matches("deviceSelect"));
    actor.send({ type: "SELECT_DEVICES", devices: DEFAULT_DEVICES });
    await waitFor(actor, (s) => s.matches("preview"));
    actor.send({ type: "JOIN" });
    actor.send({ type: "LEAVE" });
    await waitFor(actor, (s) => s.matches("ended"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CAST_EVENTS.ending);
    expect(events).toContain(CAST_EVENTS.ended);
    const endedCall = track.mock.calls.find((c) => c[0] === CAST_EVENTS.ended);
    expect(endedCall?.[1]).toMatchObject({ stub: true });
  });
});

describe("castMachine \u{2014} invalid token path", () => {
  it("a bad/expired token routes to invalid and fires cast_invalid_token", async () => {
    const track = vi.fn();
    const actor = createActor(castMachine, {
      input: inputFor(badResolve, grant, track),
    }).start();

    await waitFor(actor, (s) => s.matches("invalid"));
    expect(actor.getSnapshot().context.invalidReason).toBe("Streaming token has expired");

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CAST_EVENTS.tokenChecked);
    expect(events).toContain(CAST_EVENTS.invalidToken);
    expect(events).not.toContain(CAST_EVENTS.tokenValid);

    const invalidCall = track.mock.calls.find((c) => c[0] === CAST_EVENTS.invalidToken);
    expect(invalidCall?.[1]).toMatchObject({ reason: "Streaming token has expired" });
  });

  it("invalid -> RETRY re-checks the token", async () => {
    const track = vi.fn();
    let calls = 0;
    const resolveToken: ResolveTokenFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("Invalid streaming key");
      return okResolve(args);
    };
    const actor = createActor(castMachine, {
      input: inputFor(resolveToken, grant, track),
    }).start();

    await waitFor(actor, (s) => s.matches("invalid"));
    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("deviceSelect"));
    expect(actor.getSnapshot().matches("deviceSelect")).toBe(true);
  });
});

describe("castMachine \u{2014} permission denial path (guardrail)", () => {
  it("a denial stays on permissions, flags denied, and is recoverable", async () => {
    const track = vi.fn();
    let calls = 0;
    const requestPermissions: RequestPermissionsFn = async (args) => {
      calls += 1;
      if (calls === 1) return { granted: false };
      return grant(args);
    };
    const actor = createActor(castMachine, {
      input: inputFor(okResolve, requestPermissions, track),
    }).start();

    await waitFor(actor, (s) => s.matches("deviceSelect"));
    actor.send({ type: "SELECT_DEVICES", devices: DEFAULT_DEVICES });

    await waitFor(actor, (s) => s.matches("permissions") && s.context.permissionsDenied);
    let events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CAST_EVENTS.permissionsDenied);
    expect(events).not.toContain(CAST_EVENTS.permissionsGranted);

    actor.send({ type: "RETRY_PERMISSIONS" });
    await waitFor(actor, (s) => s.matches("preview"));
    expect(actor.getSnapshot().context.permissionsDenied).toBe(false);
    events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CAST_EVENTS.permissionsGranted);
  });
});

describe("castMachine \u{2014} screen-share in live (simulated LiveKit publish)", () => {
  async function toLive(track: TrackFn, shareScreen?: ShareScreenFn) {
    const actor = createActor(castMachine, {
      input: { ...inputFor(okResolve, grant, track), shareScreen },
    }).start();
    await waitFor(actor, (s) => s.matches("deviceSelect"));
    actor.send({ type: "SELECT_DEVICES", devices: DEFAULT_DEVICES });
    await waitFor(actor, (s) => s.matches("preview"));
    actor.send({ type: "JOIN" });
    await waitFor(actor, (s) => s.matches({ live: "idle" }));
    return actor;
  }

  it("TOGGLE_SCREENSHARE publishes a track and fires cast_screenshare_started", async () => {
    const track = vi.fn();
    const shareScreen: ShareScreenFn = async () => ({ published: true });
    const actor = await toLive(track, shareScreen);

    actor.send({ type: "TOGGLE_SCREENSHARE" });
    await waitFor(actor, (s) => s.matches({ live: "sharing" }));
    expect(actor.getSnapshot().context.screenSharing).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CAST_EVENTS.screenshareStarted);
    const call = track.mock.calls.find((c) => c[0] === CAST_EVENTS.screenshareStarted);
    expect(call?.[1]).toMatchObject({ stub: true });

    actor.send({ type: "TOGGLE_SCREENSHARE" });
    expect(actor.getSnapshot().matches({ live: "idle" })).toBe(true);
    expect(actor.getSnapshot().context.screenSharing).toBe(false);
    expect(actor.getSnapshot().context.screenShareFailed).toBe(false);
  });

  it("a failed publish stays live (idle), flags failure, fires cast_screenshare_failed", async () => {
    const track = vi.fn();
    const shareScreen: ShareScreenFn = async () => ({ published: false });
    const actor = await toLive(track, shareScreen);

    actor.send({ type: "TOGGLE_SCREENSHARE" });
    await waitFor(actor, (s) => s.context.screenShareFailed === true);
    expect(actor.getSnapshot().matches({ live: "idle" })).toBe(true);
    expect(actor.getSnapshot().context.screenSharing).toBe(false);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CAST_EVENTS.screenshareFailed);
    expect(events).not.toContain(CAST_EVENTS.screenshareStarted);
    const call = track.mock.calls.find((c) => c[0] === CAST_EVENTS.screenshareFailed);
    expect(call?.[1]).toMatchObject({ stub: true });
  });

  it("a rejected publish (thrown) is recoverable and fires cast_screenshare_failed", async () => {
    const track = vi.fn();
    const shareScreen: ShareScreenFn = async () => {
      throw new Error("getDisplayMedia denied");
    };
    const actor = await toLive(track, shareScreen);

    actor.send({ type: "TOGGLE_SCREENSHARE" });
    await waitFor(actor, (s) => s.context.screenShareFailed === true);
    expect(actor.getSnapshot().matches({ live: "idle" })).toBe(true);
    actor.send({ type: "LEAVE" });
    expect(actor.getSnapshot().matches("ending") || actor.getSnapshot().matches("ended")).toBe(
      true,
    );
  });

  it("simulateShareScreen publishes by default (no network)", async () => {
    expect(await simulateShareScreen({})).toEqual({ published: true });
  });
});

describe("simulateResolveToken / simulateGrant", () => {
  it("resolves faithful upstream shapes for a non-blank token (no network)", async () => {
    const r = await simulateResolveToken({ token: "abc", identity: "Eve" });
    expect(r.info.placeName).toBe("Genesis Plaza");
    expect(r.credentials.url).toMatch(/^wss:/);
    expect(r.credentials.token).toContain("SIMULATED.");
    expect(r.credentials.identity).toBe("Eve");
  });

  it("rejects a blank token (=> invalid) and an 'expired' sentinel", async () => {
    await expect(simulateResolveToken({ token: "   ", identity: "" })).rejects.toThrow(
      /Invalid streaming key/,
    );
    await expect(simulateResolveToken({ token: "expired", identity: "" })).rejects.toThrow(
      /expired/,
    );
  });

  it("simulateGrant grants by default", async () => {
    expect(await simulateGrant({ devices: DEFAULT_DEVICES })).toEqual({ granted: true });
  });
});
