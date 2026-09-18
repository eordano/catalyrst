import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";

import {
  MP_EVENTS,
  mpPanelMachine,
  type MpLaunchFn,
  type MpPanelInput,
  type MpReplayFn,
  type MpTrackFn,
} from "./machine";
import {
  GAME_FIXTURES,
  LANE_CAPS,
  SYNC_FIXTURE,
  fixtureAllowed,
  fixturesForLane,
  validateSpec,
  type MpRunSpec,
} from "@ui/creatorhub/mp/rules";

const TRACK_CTX = {
  sid: "sid-mp",
  story: "creator-hub/multiplayer-test",
  variant: "panel",
  experimentKey: "ch_mp_scene_testing",
} as const;

const SPEC: MpRunSpec = {
  lane: "protocol",
  scene: { kind: "fixture", ref: SYNC_FIXTURE },
  bots: 2,
  mode: "burst",
  shape: { profile: "clean", seed: 42 },
};

const okLaunch: MpLaunchFn = async () => ({ id: "run-1" });
const okReplay: MpReplayFn = async ({ tier }) =>
  tier === "a"
    ? { id: "rp-1", outcome: { converged_hash: "ab12cd34", decision_hash: "ef56" } }
    : { id: "run-2" };

function boot(overrides: Partial<MpPanelInput> = {}) {
  const track = vi.fn() as unknown as MpTrackFn;
  const input: MpPanelInput = {
    trackCtx: TRACK_CTX,
    track,
    launch: okLaunch,
    replay: okReplay,
    ...overrides,
  };
  const actor = createActor(mpPanelMachine, { input });
  actor.start();
  return { actor, track: input.track as ReturnType<typeof vi.fn> };
}

async function toRunning(overrides: Partial<MpPanelInput> = {}) {
  const ctx = boot(overrides);
  ctx.actor.send({ type: "PAIRED" });
  ctx.actor.send({ type: "LAUNCH", spec: SPEC });
  await waitFor(ctx.actor, (s) => s.matches("running"));
  return ctx;
}

async function toReviewing(overrides: Partial<MpPanelInput> = {}) {
  const ctx = await toRunning(overrides);
  ctx.actor.send({ type: "STATUS", state: "done" });
  return ctx;
}

describe("lane rules", () => {
  it("caps protocol at 2-8 and engine/mixed at 2-3, and gates game fixtures to the protocol lane (mp-sync everywhere)", () => {
    expect(LANE_CAPS.protocol).toEqual({ min: 2, max: 8 });
    expect(LANE_CAPS.engine).toEqual({ min: 2, max: 3 });
    expect(LANE_CAPS.mixed).toEqual({ min: 2, max: 3 });

    expect(validateSpec({ ...SPEC, bots: 8 }).ok).toBe(true);
    expect(validateSpec({ ...SPEC, bots: 9 }).ok).toBe(false);
    expect(validateSpec({ ...SPEC, bots: 1 }).ok).toBe(false);
    for (const lane of ["engine", "mixed"] as const) {
      expect(validateSpec({ ...SPEC, lane, bots: 3 }).ok).toBe(true);
      const over = validateSpec({ ...SPEC, lane, bots: 4 });
      expect(over.ok).toBe(false);
      if (!over.ok) expect(over.field).toBe("bots");
    }

    for (const g of GAME_FIXTURES) {
      expect(fixtureAllowed("protocol", g)).toBe(true);
      expect(fixtureAllowed("engine", g)).toBe(false);
      expect(fixtureAllowed("mixed", g)).toBe(false);
    }
    for (const lane of ["protocol", "engine", "mixed"] as const) {
      expect(fixtureAllowed(lane, SYNC_FIXTURE)).toBe(true);
    }
    expect(fixturesForLane("engine")).toEqual([SYNC_FIXTURE]);
    expect(fixturesForLane("protocol")).toContain("fastlane");

    const bad = validateSpec({
      ...SPEC,
      lane: "engine",
      bots: 2,
      scene: { kind: "fixture", ref: "fastlane" },
    });
    expect(bad.ok).toBe(false);
    if (!bad.ok) expect(bad.field).toBe("scene");
  });

  it("accepts presets and rejects malformed thresholds", () => {
    expect(validateSpec({ preset: "smoke-2bot-burst" }).ok).toBe(true);
    expect(validateSpec({ preset: " " }).ok).toBe(false);
    expect(
      validateSpec({
        ...SPEC,
        thresholds: {
          max_diverged_final: 0,
          max_converge_ms: -1,
          min_synced_pct: 100,
          max_never_synced: 0,
        },
      }).ok,
    ).toBe(false);
    expect(
      validateSpec({
        ...SPEC,
        thresholds: {
          max_diverged_final: 0,
          max_converge_ms: 10000,
          min_synced_pct: 101,
          max_never_synced: 0,
        },
      }).ok,
    ).toBe(false);
  });
});

describe("mpPanelMachine \u{2014} pairing", () => {
  it("boots unpaired; PAIRED reaches idle and tracks it; PAIR_LOST drops a running run back to unpaired", async () => {
    const { actor, track } = boot();
    expect(actor.getSnapshot().value).toBe("unpaired");
    actor.send({ type: "PAIRED" });
    expect(actor.getSnapshot().value).toBe("idle");
    expect(track).toHaveBeenCalledWith(MP_EVENTS.paired, {}, TRACK_CTX);

    actor.send({ type: "LAUNCH", spec: SPEC });
    await waitFor(actor, (s) => s.matches("running"));
    actor.send({ type: "PAIR_LOST" });
    expect(actor.getSnapshot().value).toBe("unpaired");
  });
});

describe("mpPanelMachine \u{2014} launch", () => {
  it("ignores LAUNCH with an over-cap or gated spec; a valid LAUNCH calls the sidecar, stores the run id, tracks the spec", async () => {
    const launch = vi.fn(okLaunch);
    const { actor, track } = boot({ launch });
    actor.send({ type: "PAIRED" });
    actor.send({ type: "LAUNCH", spec: { ...SPEC, lane: "engine", bots: 4 } });
    actor.send({
      type: "LAUNCH",
      spec: { ...SPEC, lane: "mixed", bots: 2, scene: { kind: "fixture", ref: "flagtag" } },
    });
    expect(actor.getSnapshot().value).toBe("idle");
    expect(launch).not.toHaveBeenCalled();

    actor.send({ type: "LAUNCH", spec: SPEC });
    await waitFor(actor, (s) => s.matches("running"));
    expect(launch).toHaveBeenCalledWith(SPEC);
    expect(actor.getSnapshot().context.runId).toBe("run-1");
    expect(actor.getSnapshot().context.phase).toBe("queued");
    expect(track).toHaveBeenCalledWith(
      MP_EVENTS.launched,
      {
        lane: "protocol",
        bots: 2,
        mode: "burst",
        source_kind: "fixture",
        profile: "clean",
      },
      TRACK_CTX,
    );
  });

  it("launch rejection returns to idle with the error", async () => {
    const launch: MpLaunchFn = async () => {
      throw new Error("scene dir not found");
    };
    const { actor, track } = boot({ launch });
    actor.send({ type: "PAIRED" });
    actor.send({ type: "LAUNCH", spec: SPEC });
    await waitFor(actor, (s) => s.matches("idle") && !!s.context.lastError);
    expect(actor.getSnapshot().context.lastError).toBe("scene dir not found");
    expect(track).toHaveBeenCalledWith(
      MP_EVENTS.rejected,
      { error: "scene dir not found" },
      TRACK_CTX,
    );
  });
});

describe("mpPanelMachine \u{2014} run lifecycle", () => {
  it("walks starting \u{2192} running \u{2192} analyzing inside running, then STATUS done lands in reviewing and tracks completion", async () => {
    const { actor, track } = await toRunning();
    for (const state of ["starting", "running", "analyzing"] as const) {
      actor.send({ type: "STATUS", state, detail: `in ${state}` });
      expect(actor.getSnapshot().value).toBe("running");
      expect(actor.getSnapshot().context.phase).toBe(state);
      expect(actor.getSnapshot().context.detail).toBe(`in ${state}`);
    }

    actor.send({ type: "STATUS", state: "done" });
    expect(actor.getSnapshot().matches({ reviewing: "report" })).toBe(true);
    expect(track).toHaveBeenCalledWith(
      MP_EVENTS.completed,
      { run: "run-1" },
      TRACK_CTX,
    );
  });

  it("STATUS failed lands back in idle with the detail as error; STOP forwards to the sidecar and stays running", async () => {
    const { actor, track } = await toRunning();
    actor.send({ type: "STATUS", state: "failed", detail: "livekit died" });
    expect(actor.getSnapshot().value).toBe("idle");
    expect(actor.getSnapshot().context.lastError).toBe("livekit died");
    expect(track).toHaveBeenCalledWith(
      MP_EVENTS.failed,
      { run: "run-1", detail: "livekit died" },
      TRACK_CTX,
    );

    const stop = vi.fn();
    const ctx = await toRunning({ stop });
    ctx.actor.send({ type: "STOP" });
    expect(stop).toHaveBeenCalledWith("run-1");
    expect(ctx.actor.getSnapshot().value).toBe("running");
  });
});

describe("mpPanelMachine \u{2014} reviewing + replay substate", () => {
  it("Tier A replay resolves an outcome hash; SELECT_RUN reviews an old run; CLOSE_REPLAY returns to the report; NEW_RUN clears the run", async () => {
    const { actor, track } = await toReviewing();
    actor.send({ type: "OPEN_REPLAY" });
    expect(actor.getSnapshot().matches({ reviewing: { replay: "form" } })).toBe(true);
    actor.send({ type: "REPLAY", tier: "a", profile: "latency150", seed: 42 });
    await waitFor(actor, (s) => s.matches({ reviewing: { replay: "result" } }));
    const out = actor.getSnapshot().context.replayOutcome;
    expect(out?.tier).toBe("a");
    expect(out?.hash).toBe("ab12cd34");
    expect(out?.decisionHash).toBe("ef56");
    expect(track).toHaveBeenCalledWith(
      MP_EVENTS.replayRequested,
      { run: "run-1", tier: "a" },
      TRACK_CTX,
    );
    expect(track).toHaveBeenCalledWith(
      MP_EVENTS.replayCompleted,
      { run: "run-1", tier: "a" },
      TRACK_CTX,
    );

    const old = boot();
    old.actor.send({ type: "PAIRED" });
    old.actor.send({ type: "SELECT_RUN", runId: "run-9" });
    expect(old.actor.getSnapshot().matches({ reviewing: "report" })).toBe(true);
    expect(old.actor.getSnapshot().context.runId).toBe("run-9");

    old.actor.send({ type: "OPEN_REPLAY" });
    old.actor.send({ type: "CLOSE_REPLAY" });
    expect(old.actor.getSnapshot().matches({ reviewing: "report" })).toBe(true);
    old.actor.send({ type: "NEW_RUN" });
    expect(old.actor.getSnapshot().value).toBe("idle");
    expect(old.actor.getSnapshot().context.runId).toBeNull();
    expect(old.actor.getSnapshot().context.replayOutcome).toBeNull();
  });

  it("Tier B replay surfaces the new live run id; a replay failure returns to the form with the error", async () => {
    const { actor } = await toReviewing();
    actor.send({ type: "OPEN_REPLAY" });
    actor.send({ type: "REPLAY", tier: "b" });
    await waitFor(actor, (s) => s.matches({ reviewing: { replay: "result" } }));
    const out = actor.getSnapshot().context.replayOutcome;
    expect(out?.tier).toBe("b");
    expect(out?.id).toBe("run-2");
    expect(out?.hash).toBeNull();

    const replay: MpReplayFn = async () => {
      throw new Error("bundle missing frames");
    };
    const failing = await toReviewing({ replay });
    failing.actor.send({ type: "OPEN_REPLAY" });
    failing.actor.send({ type: "REPLAY", tier: "a" });
    await waitFor(failing.actor, (s) => s.matches({ reviewing: { replay: "form" } }));
    expect(failing.actor.getSnapshot().context.replayError).toBe("bundle missing frames");
  });
});
