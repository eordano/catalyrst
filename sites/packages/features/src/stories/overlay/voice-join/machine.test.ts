import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  voiceMachine,
  VOICE_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveVoiceSnapshot,
  slugToState,
  slugIsMuted,
  stateToSlug,
  simulateConnect,
  type ConnectFn,
  type ConnectResult,
  type TrackFn,
} from "./machine";

const RESULT: ConnectResult = {
  connectionUrl: "livekit:wss://test?access_token=STUB",
  roomName: "voice-chat-private-test",
};

const okConnect: ConnectFn = async () => RESULT;
const failConnect: ConnectFn = async () => {
  throw new Error("livekit unreachable");
};

function inputFor(connect: ConnectFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "bevy-overlay-voice-join",
      variant: "wizard",
      experimentKey: "cl_voice_join",
    },
    connect,
    track,
    roomId: "call-test",
  };
}

const EXPECTED_STATES = new Set([
  "resting",
  "requesting",
  "connecting",
  "talking",
  "left",
  "failed",
]);

const TRAVERSAL_EVENTS = [
  { type: "REQUEST" as const, kind: "private" as const },
  { type: "REQUEST" as const, kind: "community" as const },
  { type: "TOGGLE_MUTE" as const },
  { type: "LEAVE" as const },
  { type: "RETRY" as const },
];

describe("voiceMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(voiceMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);
  });

  it("each state mirrors out to a slug that round-trips back to it", () => {
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
    }
  });

  it("the `mute` alias deep-links into talking and flags pre-mute", () => {
    expect(SLUG_TO_STATE.mute).toBe("talking");
    expect(slugToState("mute")).toBe("talking");
    expect(slugIsMuted("mute")).toBe(true);
    expect(slugIsMuted("talk")).toBe(false);
    expect(slugIsMuted(null)).toBe(false);
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.resting);
    expect(slugToState(null)).toBe("resting");
    expect(slugToState(undefined)).toBe("resting");
    expect(slugToState("")).toBe("resting");
    expect(slugToState("nope")).toBe("resting");
    expect(slugToState("request")).toBe("requesting");
    expect(slugToState("token")).toBe("connecting");
    expect(slugToState("talk")).toBe("talking");
    expect(slugToState("leave")).toBe("left");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("voiceMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveVoiceSnapshot({
      step: "resting",
      trackCtx: inputFor(okConnect, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating connecting does NOT fire telemetry and does NOT auto-connect", async () => {
    const track = vi.fn();
    const connect = vi.fn(okConnect);
    const snapshot = resolveVoiceSnapshot({
      step: "connecting",
      trackCtx: inputFor(connect, track).trackCtx,
      connect,
      track,
    });
    const actor = createActor(voiceMachine, {
      input: inputFor(connect, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("connecting")).toBe(true);
    expect(actor.getSnapshot().context.kind).toBe("private");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(connect).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("connecting")).toBe(true);
  });

  it("hydrating talking pre-muted does not fire telemetry and seeds micMuted", () => {
    const track = vi.fn();
    const snapshot = resolveVoiceSnapshot({
      step: "talking",
      trackCtx: inputFor(okConnect, track).trackCtx,
      track,
      muted: true,
    });
    const actor = createActor(voiceMachine, {
      input: inputFor(okConnect, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("talking")).toBe(true);
    expect(actor.getSnapshot().context.micMuted).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveVoiceSnapshot({
      step: "talking",
      trackCtx: inputFor(okConnect, track).trackCtx,
      track,
    });
    const actor = createActor(voiceMachine, {
      input: inputFor(okConnect, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("talking")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "TOGGLE_MUTE" });
    expect(actor.getSnapshot().context.micMuted).toBe(true);
    const muteCall = track.mock.calls.find(
      (c) => c[0] === VOICE_EVENTS.muteToggled,
    );
    expect(muteCall).toBeDefined();
    expect(muteCall?.[1]).toMatchObject({ muted: true });
  });
});

describe("voiceMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(voiceMachine, {
      input: inputFor(okConnect, () => {}),
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
  });

  it("reaching connecting passes through REQUEST", () => {
    const paths = getShortestPaths(voiceMachine, {
      input: inputFor(okConnect, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const connecting = paths.find(
      (p) => (p.state.value as string) === "connecting",
    );
    expect(connecting).toBeDefined();
    const events = connecting!.steps.map((s) => s.event.type);
    expect(events).toContain("REQUEST");
  });
});

describe("voiceMachine \u{2014} telemetry events (happy path)", () => {
  it("open -> request -> token -> talk -> mute -> leave fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(voiceMachine, {
      input: inputFor(okConnect, track),
    }).start();

    expect(track.mock.calls.map((c) => c[0])).toContain(
      VOICE_EVENTS.widgetOpened,
    );
    expect(actor.getSnapshot().matches("resting")).toBe(true);

    actor.send({ type: "REQUEST", kind: "private" });
    await waitFor(actor, (s) => s.matches("talking"));

    actor.send({ type: "TOGGLE_MUTE" });
    expect(actor.getSnapshot().context.micMuted).toBe(true);
    actor.send({ type: "TOGGLE_MUTE" });
    expect(actor.getSnapshot().context.micMuted).toBe(false);

    actor.send({ type: "LEAVE" });
    await waitFor(actor, (s) => s.matches("left"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(VOICE_EVENTS.widgetOpened);
    expect(events).toContain(VOICE_EVENTS.sessionRequested);
    expect(events).toContain(VOICE_EVENTS.tokenIssued);
    expect(events).toContain(VOICE_EVENTS.join);
    expect(events).toContain(VOICE_EVENTS.muteToggled);
    expect(events).toContain(VOICE_EVENTS.left);

    expect(events.indexOf(VOICE_EVENTS.widgetOpened)).toBeLessThan(
      events.indexOf(VOICE_EVENTS.sessionRequested),
    );
    expect(events.indexOf(VOICE_EVENTS.tokenIssued)).toBeLessThan(
      events.indexOf(VOICE_EVENTS.join),
    );
    expect(events.indexOf(VOICE_EVENTS.join)).toBeLessThan(
      events.indexOf(VOICE_EVENTS.left),
    );

    const reqCall = track.mock.calls.find(
      (c) => c[0] === VOICE_EVENTS.sessionRequested,
    );
    expect(reqCall?.[1]).toMatchObject({ kind: "private" });
    expect(reqCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "cl_voice_join",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("community request carries kind=community through token + join", async () => {
    const track = vi.fn();
    const actor = createActor(voiceMachine, {
      input: inputFor(okConnect, track),
    }).start();

    actor.send({ type: "REQUEST", kind: "community" });
    await waitFor(actor, (s) => s.matches("talking"));

    const tokenCall = track.mock.calls.find(
      (c) => c[0] === VOICE_EVENTS.tokenIssued,
    );
    expect(tokenCall?.[1]).toMatchObject({ kind: "community", stub: true });
  });

  it("mute toggle reports the post-toggle muted value each time", () => {
    const track = vi.fn();
    const snapshot = resolveVoiceSnapshot({
      step: "talking",
      trackCtx: inputFor(okConnect, track).trackCtx,
      track,
    });
    const actor = createActor(voiceMachine, {
      input: inputFor(okConnect, track),
      snapshot,
    }).start();

    actor.send({ type: "TOGGLE_MUTE" });
    actor.send({ type: "TOGGLE_MUTE" });
    const muteCalls = track.mock.calls.filter(
      (c) => c[0] === VOICE_EVENTS.muteToggled,
    );
    expect(muteCalls.map((c) => c[1].muted)).toEqual([true, false]);
  });
});

describe("voiceMachine \u{2014} connect failure + retry", () => {
  it("connect error -> session_failed -> RETRY recovers to talking", async () => {
    const track = vi.fn();
    let calls = 0;
    const connect: ConnectFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("livekit unreachable");
      return okConnect(args);
    };

    const actor = createActor(voiceMachine, {
      input: inputFor(connect, track),
    }).start();

    actor.send({ type: "REQUEST", kind: "private" });
    await waitFor(actor, (s) => s.matches("failed"));
    expect(actor.getSnapshot().context.error).toBe("livekit unreachable");
    expect(track.mock.calls.map((c) => c[0])).toContain(
      VOICE_EVENTS.sessionFailed,
    );

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("talking"));
    expect(track.mock.calls.map((c) => c[0])).toContain(VOICE_EVENTS.join);
  });

  it("failed -> LEAVE ends the session without talking", async () => {
    const track = vi.fn();
    const actor = createActor(voiceMachine, {
      input: inputFor(failConnect, track),
    }).start();

    actor.send({ type: "REQUEST", kind: "private" });
    await waitFor(actor, (s) => s.matches("failed"));

    actor.send({ type: "LEAVE" });
    await waitFor(actor, (s) => s.matches("left"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(VOICE_EVENTS.left);
    expect(events).not.toContain(VOICE_EVENTS.join);
  });
});

describe("simulateConnect", () => {
  it("resolves a faithful connection_url + room name (no network)", async () => {
    const priv = await simulateConnect({ kind: "private", roomId: "abc" });
    const comm = await simulateConnect({ kind: "community", roomId: "xyz" });
    expect(priv.roomName).toBe("voice-chat-private-abc");
    expect(priv.connectionUrl).toContain("livekit:wss://");
    expect(comm.roomName).toBe("voice-chat-community-xyz");
  });
});
