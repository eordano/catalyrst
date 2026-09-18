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

const TRAVERSAL_EVENTS = [
  { type: "REQUEST" as const, kind: "private" as const },
  { type: "REQUEST" as const, kind: "community" as const },
  { type: "TOGGLE_MUTE" as const },
  { type: "LEAVE" as const },
  { type: "RETRY" as const },
];

function hydratedTalking(track: TrackFn, muted?: boolean) {
  return createActor(voiceMachine, {
    input: inputFor(okConnect, track),
    snapshot: resolveVoiceSnapshot({
      step: "talking",
      trackCtx: inputFor(okConnect, track).trackCtx,
      track,
      ...(muted === undefined ? {} : { muted }),
    }),
  }).start();
}

describe("voiceMachine \u{2014} URL ?step slug map", () => {
  it("covers exactly the machine's states with slugs that round-trip and fall back to the first step", () => {
    const machineStates = new Set(Object.keys(voiceMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.resting);
    for (const missing of [null, undefined, "", "nope"]) expect(slugToState(missing)).toBe("resting");
    expect(slugToState("request")).toBe("requesting");
    expect(slugToState("token")).toBe("connecting");
    expect(slugToState("talk")).toBe("talking");
    expect(slugToState("leave")).toBe("left");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });

  it("the `mute` alias deep-links into talking and flags pre-mute", () => {
    expect(SLUG_TO_STATE.mute).toBe("talking");
    expect(slugToState("mute")).toBe("talking");
    expect(slugIsMuted("mute")).toBe(true);
    expect(slugIsMuted("talk")).toBe(false);
    expect(slugIsMuted(null)).toBe(false);
  });
});

describe("voiceMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("pins the step and seeds kind/micMuted without telemetry or an auto-connect", async () => {
    const track = vi.fn();
    const connect = vi.fn(okConnect);
    expect(
      resolveVoiceSnapshot({ step: "resting", trackCtx: inputFor(connect, track).trackCtx }),
    ).toBeUndefined();

    const connecting = createActor(voiceMachine, {
      input: inputFor(connect, track),
      snapshot: resolveVoiceSnapshot({
        step: "connecting",
        trackCtx: inputFor(connect, track).trackCtx,
        connect,
        track,
      }),
    }).start();
    expect(connecting.getSnapshot().matches("connecting")).toBe(true);
    expect(connecting.getSnapshot().context.kind).toBe("private");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(connect).not.toHaveBeenCalled();
    expect(connecting.getSnapshot().matches("connecting")).toBe(true);

    const talking = hydratedTalking(track, true);
    expect(talking.getSnapshot().matches("talking")).toBe(true);
    expect(talking.getSnapshot().context.micMuted).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });

  it("real transitions after hydration still fire telemetry, reporting the post-toggle muted value each time", () => {
    const track = vi.fn();
    const actor = hydratedTalking(track);
    expect(actor.getSnapshot().matches("talking")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "TOGGLE_MUTE" });
    expect(actor.getSnapshot().context.micMuted).toBe(true);
    actor.send({ type: "TOGGLE_MUTE" });
    expect(actor.getSnapshot().context.micMuted).toBe(false);
    const muteCalls = track.mock.calls.filter((c) => c[0] === VOICE_EVENTS.muteToggled);
    expect(muteCalls.map((c) => c[1].muted)).toEqual([true, false]);
  });
});

describe("voiceMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("the funnel states are event-reachable and connecting is reached through REQUEST", () => {
    const paths = getShortestPaths(voiceMachine, {
      input: inputFor(okConnect, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) ends.add(p.state.value as string);
    expect(ends.has("connecting")).toBe(true);

    const connecting = paths.find((p) => (p.state.value as string) === "connecting");
    expect(connecting).toBeDefined();
    expect(connecting!.steps.map((s) => s.event.type)).toContain("REQUEST");
  });
});

describe("voiceMachine \u{2014} telemetry events (happy path)", () => {
  it("open -> request -> token -> talk -> mute -> leave fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(voiceMachine, {
      input: inputFor(okConnect, track),
    }).start();

    expect(track.mock.calls.map((c) => c[0])).toContain(VOICE_EVENTS.widgetOpened);
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
    for (const event of [
      VOICE_EVENTS.widgetOpened,
      VOICE_EVENTS.sessionRequested,
      VOICE_EVENTS.tokenIssued,
      VOICE_EVENTS.join,
      VOICE_EVENTS.muteToggled,
      VOICE_EVENTS.left,
    ]) {
      expect(events, event).toContain(event);
    }
    expect(events.indexOf(VOICE_EVENTS.widgetOpened)).toBeLessThan(events.indexOf(VOICE_EVENTS.sessionRequested));
    expect(events.indexOf(VOICE_EVENTS.tokenIssued)).toBeLessThan(events.indexOf(VOICE_EVENTS.join));
    expect(events.indexOf(VOICE_EVENTS.join)).toBeLessThan(events.indexOf(VOICE_EVENTS.left));

    const reqCall = track.mock.calls.find((c) => c[0] === VOICE_EVENTS.sessionRequested);
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

    const tokenCall = track.mock.calls.find((c) => c[0] === VOICE_EVENTS.tokenIssued);
    expect(tokenCall?.[1]).toMatchObject({ kind: "community", stub: true });
  });
});

describe("voiceMachine \u{2014} connect failure", () => {
  it("a failed connect fires session_failed, then RETRY recovers to talking or LEAVE ends the session unjoined", async () => {
    const retried = vi.fn();
    let calls = 0;
    const connect: ConnectFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("livekit unreachable");
      return okConnect(args);
    };
    const retry = createActor(voiceMachine, {
      input: inputFor(connect, retried),
    }).start();
    retry.send({ type: "REQUEST", kind: "private" });
    await waitFor(retry, (s) => s.matches("failed"));
    expect(retry.getSnapshot().context.error).toBe("livekit unreachable");
    expect(retried.mock.calls.map((c) => c[0])).toContain(VOICE_EVENTS.sessionFailed);
    retry.send({ type: "RETRY" });
    await waitFor(retry, (s) => s.matches("talking"));
    expect(retried.mock.calls.map((c) => c[0])).toContain(VOICE_EVENTS.join);

    const left = vi.fn();
    const leave = createActor(voiceMachine, {
      input: inputFor(failConnect, left),
    }).start();
    leave.send({ type: "REQUEST", kind: "private" });
    await waitFor(leave, (s) => s.matches("failed"));
    leave.send({ type: "LEAVE" });
    await waitFor(leave, (s) => s.matches("left"));
    const events = left.mock.calls.map((c) => c[0]);
    expect(events).toContain(VOICE_EVENTS.left);
    expect(events).not.toContain(VOICE_EVENTS.join);
  });
});
