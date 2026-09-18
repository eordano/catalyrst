import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";

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
const grant: RequestPermissionsFn = async () => ({ granted: true });
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

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

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

describe("castMachine \u{2014} URL ?step slug map", () => {
  it("uses the audit-spec kebab-case step names, unique and round-tripping, and falls back to token-check", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(castMachine.states)));
    expect(slugToState("token-check")).toBe("tokenCheck");
    expect(slugToState("device-select")).toBe("deviceSelect");
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.tokenCheck);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("tokenCheck");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("castMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots token-check without a snapshot, hydrates later steps silently, and only real transitions track", async () => {
    const track = vi.fn();
    const resolveToken = vi.fn(okResolve);
    const requestPermissions = vi.fn(grant);
    const trackCtx = inputFor(resolveToken, requestPermissions, track).trackCtx;
    expect(resolveCastSnapshot({ step: "tokenCheck", trackCtx, token: "k" })).toBeUndefined();

    const preview = createActor(castMachine, {
      input: inputFor(resolveToken, requestPermissions, track),
      snapshot: resolveCastSnapshot({
        step: "preview",
        trackCtx,
        token: "k",
        resolveToken,
        requestPermissions,
        track,
      }),
    }).start();
    expect(preview.getSnapshot().matches("preview")).toBe(true);
    expect(preview.getSnapshot().context.info?.placeName).toBeTruthy();
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(resolveToken).not.toHaveBeenCalled();
    expect(requestPermissions).not.toHaveBeenCalled();
    expect(preview.getSnapshot().matches("preview")).toBe(true);

    const live = createActor(castMachine, {
      input: inputFor(okResolve, grant, track),
      snapshot: resolveCastSnapshot({ step: "live", trackCtx, token: "k", track }),
    }).start();
    expect(live.getSnapshot().matches("live")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    live.send({ type: "LEAVE" });
    expect(names(track)).toContain(CAST_EVENTS.ending);
  });
});

describe("castMachine \u{2014} happy path (token -> devices -> permissions -> preview -> live -> ended)", () => {
  it("fires the full funnel through live, then LEAVE fires teardown telemetry through ended", async () => {
    const track = vi.fn();
    const actor = createActor(castMachine, {
      input: inputFor(okResolve, grant, track),
    }).start();

    await waitFor(actor, (s) => s.matches("deviceSelect"));
    actor.send({ type: "SELECT_DEVICES", devices: DEFAULT_DEVICES });
    await waitFor(actor, (s) => s.matches("preview"));
    actor.send({ type: "JOIN" });
    expect(actor.getSnapshot().matches("live")).toBe(true);

    let events = names(track);
    for (const e of [
      CAST_EVENTS.tokenChecked,
      CAST_EVENTS.tokenValid,
      CAST_EVENTS.devicesSelected,
      CAST_EVENTS.permissionsGranted,
      CAST_EVENTS.previewReady,
      CAST_EVENTS.joinRequested,
      CAST_EVENTS.wentLive,
    ]) {
      expect(events).toContain(e);
    }
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

    actor.send({ type: "LEAVE" });
    await waitFor(actor, (s) => s.matches("ended"));
    events = names(track);
    expect(events).toContain(CAST_EVENTS.ending);
    expect(events).toContain(CAST_EVENTS.ended);
    const endedCall = track.mock.calls.find((c) => c[0] === CAST_EVENTS.ended);
    expect(endedCall?.[1]).toMatchObject({ stub: true });
  });
});

describe("castMachine \u{2014} invalid token path", () => {
  it("a bad/expired token routes to invalid with the reason, and RETRY re-checks it", async () => {
    const track = vi.fn();
    let calls = 0;
    const resolveToken: ResolveTokenFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("Streaming token has expired");
      return okResolve(args);
    };
    const actor = createActor(castMachine, {
      input: inputFor(resolveToken, grant, track),
    }).start();

    await waitFor(actor, (s) => s.matches("invalid"));
    expect(actor.getSnapshot().context.invalidReason).toBe("Streaming token has expired");
    const events = names(track);
    expect(events).toContain(CAST_EVENTS.tokenChecked);
    expect(events).toContain(CAST_EVENTS.invalidToken);
    expect(events).not.toContain(CAST_EVENTS.tokenValid);
    const invalidCall = track.mock.calls.find((c) => c[0] === CAST_EVENTS.invalidToken);
    expect(invalidCall?.[1]).toMatchObject({ reason: "Streaming token has expired" });

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
    expect(names(track)).toContain(CAST_EVENTS.permissionsDenied);
    expect(names(track)).not.toContain(CAST_EVENTS.permissionsGranted);

    actor.send({ type: "RETRY_PERMISSIONS" });
    await waitFor(actor, (s) => s.matches("preview"));
    expect(actor.getSnapshot().context.permissionsDenied).toBe(false);
    expect(names(track)).toContain(CAST_EVENTS.permissionsGranted);
  });
});

describe("castMachine \u{2014} screen-share in live (simulated LiveKit publish)", () => {
  it("TOGGLE_SCREENSHARE publishes then unpublishes; a failed or thrown publish stays live idle and is recoverable", async () => {
    const okTrack = vi.fn();
    const sharing = await toLive(okTrack, async () => ({ published: true }));
    sharing.send({ type: "TOGGLE_SCREENSHARE" });
    await waitFor(sharing, (s) => s.matches({ live: "sharing" }));
    expect(sharing.getSnapshot().context.screenSharing).toBe(true);
    expect(names(okTrack)).toContain(CAST_EVENTS.screenshareStarted);
    const startedCall = okTrack.mock.calls.find((c) => c[0] === CAST_EVENTS.screenshareStarted);
    expect(startedCall?.[1]).toMatchObject({ stub: true });
    sharing.send({ type: "TOGGLE_SCREENSHARE" });
    expect(sharing.getSnapshot().matches({ live: "idle" })).toBe(true);
    expect(sharing.getSnapshot().context.screenSharing).toBe(false);
    expect(sharing.getSnapshot().context.screenShareFailed).toBe(false);

    const failTrack = vi.fn();
    const failed = await toLive(failTrack, async () => ({ published: false }));
    failed.send({ type: "TOGGLE_SCREENSHARE" });
    await waitFor(failed, (s) => s.context.screenShareFailed === true);
    expect(failed.getSnapshot().matches({ live: "idle" })).toBe(true);
    expect(failed.getSnapshot().context.screenSharing).toBe(false);
    expect(names(failTrack)).toContain(CAST_EVENTS.screenshareFailed);
    expect(names(failTrack)).not.toContain(CAST_EVENTS.screenshareStarted);
    const failedCall = failTrack.mock.calls.find((c) => c[0] === CAST_EVENTS.screenshareFailed);
    expect(failedCall?.[1]).toMatchObject({ stub: true });

    const thrown = await toLive(vi.fn(), async () => {
      throw new Error("getDisplayMedia denied");
    });
    thrown.send({ type: "TOGGLE_SCREENSHARE" });
    await waitFor(thrown, (s) => s.context.screenShareFailed === true);
    expect(thrown.getSnapshot().matches({ live: "idle" })).toBe(true);
    thrown.send({ type: "LEAVE" });
    expect(
      thrown.getSnapshot().matches("ending") || thrown.getSnapshot().matches("ended"),
    ).toBe(true);
  });
});

describe("simulateResolveToken / simulateGrant / simulateShareScreen", () => {
  it("resolve faithful upstream shapes, reject blank or expired tokens, and grant/publish by default", async () => {
    const r = await simulateResolveToken({ token: "abc", identity: "Eve" });
    expect(r.info.placeName).toBeTruthy();
    expect(r.credentials.url).toMatch(/^wss:/);
    expect(r.credentials.token).toContain("SIMULATED.");
    expect(r.credentials.identity).toBe("Eve");
    await expect(simulateResolveToken({ token: "   ", identity: "" })).rejects.toThrow(
      /Invalid streaming key/,
    );
    await expect(simulateResolveToken({ token: "expired", identity: "" })).rejects.toThrow(
      /expired/,
    );
    expect(await simulateGrant({ devices: DEFAULT_DEVICES })).toEqual({ granted: true });
    expect(await simulateShareScreen({})).toEqual({ published: true });
  });
});
