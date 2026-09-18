import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  deployWorldMachine,
  DEPLOY_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveDeploySnapshot,
  slugToState,
  stateToSlug,
  type DeployFn,
  type DeployResult,
  type TrackFn,
} from "./machine";
import type { DeployFile } from "@data/lib/catalyst/creator-hub/deploy-world";

const RESULT: DeployResult = { jumpUrl: "https://catalyst.example.com/play/?realm=test.dcl.eth" };

const TRACK_CTX = {
  sid: "sid-abc",
  story: "creator-hub-deploy-scene",
  variant: "wizard",
  experimentKey: "ch_deploy_world_wizard",
} as const;

const FILES: DeployFile[] = [
  { name: "bin/index.js", size: 1_842_133 },
  { name: "scene.json", size: 1_204 },
  { name: "assets/scene/models/market-stall.glb", size: 8_930_512 },
];
const FILES_OVER_QUOTA: DeployFile[] = [
  ...FILES,
  { name: "assets/scene/video/intro-loop-4k.mp4", size: 63_882_104 },
];

const okDeploy: DeployFn = async () => RESULT;

function inputFor(
  deploy: DeployFn,
  track: TrackFn,
  extra: { namesEmpty?: boolean; files?: DeployFile[] } = {},
) {
  return {
    trackCtx: TRACK_CTX,
    deploy,
    track,
    files: extra.files ?? FILES,
    maxFileSizeMb: 50,
    namesEmpty: extra.namesEmpty ?? false,
    defaultName: "mystore.dcl.eth",
  };
}

const EXPECTED_STATES = new Set([
  "destination",
  "selectWorld",
  "namesEmpty",
  "review",
  "unavailable",
  "deploying",
  "finishing",
  "complete",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "CHOOSE_WORLDS" as const },
  { type: "PICK_NAME" as const, name: "mystore.dcl.eth" },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("deployWorldMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(deployWorldMachine.states));
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

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.destination);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("destination");
    }
    expect(slugToState("select-world")).toBe("selectWorld");
    expect(slugToState("select-empty")).toBe("namesEmpty");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("deployWorldMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step boots from initial; review and deploying hydrate without telemetry or auto-deploy; selectWorld then PICK_NAME fires", async () => {
    const track = vi.fn();
    const deploy = vi.fn(okDeploy);
    const input = inputFor(deploy, track);

    expect(resolveDeploySnapshot({ step: "destination", trackCtx: input.trackCtx })).toBeUndefined();

    const review = createActor(deployWorldMachine, {
      input,
      snapshot: resolveDeploySnapshot({
        step: "review",
        trackCtx: input.trackCtx,
        files: FILES,
        deploy,
        track,
      }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(deploy).not.toHaveBeenCalled();
    expect(review.getSnapshot().matches("review")).toBe(true);

    const deploying = createActor(deployWorldMachine, {
      input,
      snapshot: resolveDeploySnapshot({ step: "deploying", trackCtx: input.trackCtx, deploy, track }),
    }).start();
    expect(deploying.getSnapshot().matches("deploying")).toBe(true);
    await Promise.resolve();
    expect(deploy).not.toHaveBeenCalled();
    expect(track).not.toHaveBeenCalled();

    const selecting = createActor(deployWorldMachine, {
      input,
      snapshot: resolveDeploySnapshot({ step: "selectWorld", trackCtx: input.trackCtx, track }),
    }).start();
    expect(selecting.getSnapshot().matches("selectWorld")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    selecting.send({ type: "PICK_NAME", name: "gallery.dcl.eth" });
    expect(selecting.getSnapshot().matches("review")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(DEPLOY_EVENTS.nameSelected);
    expect(events).toContain(DEPLOY_EVENTS.reviewReached);
    expect(selecting.getSnapshot().context.name).toBe("gallery.dcl.eth");
  });
});

describe("deployWorldMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every path ends in an expected state, deploying needs PICK_NAME + CONFIRM, and the empty-NAMEs branch exists only when seeded", () => {
    const paths = getShortestPaths(deployWorldMachine, {
      input: inputFor(okDeploy, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    for (const s of ["selectWorld", "review", "deploying"]) {
      expect(ends.has(s)).toBe(true);
    }

    const deploying = paths.find((p) => (p.state.value as string) === "deploying");
    expect(deploying).toBeDefined();
    expect(deploying!.steps.map((s) => s.event.type)).toEqual(
      expect.arrayContaining(["PICK_NAME", "CONFIRM"]),
    );

    const emptyPaths = getShortestPaths(deployWorldMachine, {
      input: inputFor(okDeploy, () => {}, { namesEmpty: true }),
      events: TRAVERSAL_EVENTS,
    });
    const emptyEnds = new Set(emptyPaths.map((p) => p.state.value as string));
    expect(emptyEnds.has("namesEmpty")).toBe(true);
    expect(emptyEnds.has("selectWorld")).toBe(false);
    expect(emptyEnds.has("review")).toBe(false);
  });
});

describe("deployWorldMachine \u{2014} telemetry events (happy path)", () => {
  it("destination -> select -> review -> deploy -> finish -> complete fires the full funnel with one real deploy", async () => {
    const track = vi.fn();
    const deploy = vi.fn(okDeploy);
    const actor = createActor(deployWorldMachine, {
      input: inputFor(deploy, track),
    }).start();

    actor.send({ type: "CHOOSE_WORLDS" });
    expect(actor.getSnapshot().matches("selectWorld")).toBe(true);

    actor.send({ type: "PICK_NAME", name: "mystore.dcl.eth" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("complete"));

    expect(deploy).toHaveBeenCalledTimes(1);
    expect(actor.getSnapshot().context.quotaError).toBeUndefined();

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        DEPLOY_EVENTS.started,
        DEPLOY_EVENTS.destinationSelected,
        DEPLOY_EVENTS.nameSelected,
        DEPLOY_EVENTS.reviewReached,
        DEPLOY_EVENTS.confirmReached,
        DEPLOY_EVENTS.completed,
      ]),
    );
    expect(events).not.toContain(DEPLOY_EVENTS.quotaExceeded);
    expect(events.indexOf(DEPLOY_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(DEPLOY_EVENTS.completed),
    );

    const completed = track.mock.calls.find((c) => c[0] === DEPLOY_EVENTS.completed);
    expect(completed?.[1]).toMatchObject({ name: "mystore.dcl.eth" });
    expect(String(completed?.[1]?.jump_url)).toContain("realm=");
    expect(completed?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "ch_deploy_world_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("without a real deploy wired CONFIRM lands on unavailable; the empty-NAMEs path fires names_empty and never deploys", () => {
    const track = vi.fn();
    const actor = createActor(deployWorldMachine, {
      input: {
        trackCtx: TRACK_CTX,
        track,
        files: FILES,
        maxFileSizeMb: 50,
        namesEmpty: false,
        defaultName: "mystore.dcl.eth",
      },
    }).start();

    actor.send({ type: "CHOOSE_WORLDS" });
    actor.send({ type: "PICK_NAME", name: "mystore.dcl.eth" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("unavailable")).toBe(true);
    expect(actor.getSnapshot().context.result).toBeUndefined();
    expect(track.mock.calls.map((c) => c[0])).not.toContain(DEPLOY_EVENTS.completed);

    const emptyTrack = vi.fn();
    const deploy = vi.fn(okDeploy);
    const empty = createActor(deployWorldMachine, {
      input: inputFor(deploy, emptyTrack, { namesEmpty: true }),
    }).start();

    empty.send({ type: "CHOOSE_WORLDS" });
    expect(empty.getSnapshot().matches("namesEmpty")).toBe(true);

    const emptyEvents = emptyTrack.mock.calls.map((c) => c[0]);
    expect(emptyEvents).toContain(DEPLOY_EVENTS.started);
    expect(emptyEvents).toContain(DEPLOY_EVENTS.namesEmpty);
    expect(emptyEvents).not.toContain(DEPLOY_EVENTS.confirmReached);
    expect(deploy).not.toHaveBeenCalled();
  });
});

describe("deployWorldMachine \u{2014} quota guardrail blocks confirm", () => {
  it("over-quota review fires quota_exceeded, CONFIRM stays in review with quotaError and no deploy; a late SET_FILES manifest drives the same guard", () => {
    const track = vi.fn();
    const deploy = vi.fn(okDeploy);
    const actor = createActor(deployWorldMachine, {
      input: inputFor(deploy, track, { files: FILES_OVER_QUOTA }),
    }).start();

    actor.send({ type: "CHOOSE_WORLDS" });
    actor.send({ type: "PICK_NAME", name: "mystore.dcl.eth" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    const reviewCall = track.mock.calls.find((c) => c[0] === DEPLOY_EVENTS.reviewReached);
    expect(reviewCall?.[1]).toMatchObject({ exceeded: true });
    expect(track.mock.calls.map((c) => c[0])).toContain(DEPLOY_EVENTS.quotaExceeded);

    actor.send({ type: "CONFIRM" });

    const snap = actor.getSnapshot();
    expect(snap.matches("review")).toBe(true);
    expect(snap.context.quotaError).toBe(true);
    expect(snap.context.result).toBeUndefined();
    expect(deploy).not.toHaveBeenCalled();

    const quotaCalls = track.mock.calls.filter((c) => c[0] === DEPLOY_EVENTS.quotaExceeded);
    expect(quotaCalls.length).toBeGreaterThanOrEqual(2);
    expect(quotaCalls.some((c) => (c[1] as { phase?: string }).phase === "confirm")).toBe(true);

    const lateDeploy = vi.fn(okDeploy);
    const late = createActor(deployWorldMachine, {
      input: inputFor(lateDeploy, vi.fn(), { files: [] }),
    }).start();

    late.send({ type: "SET_FILES", files: FILES_OVER_QUOTA });
    late.send({ type: "PICK_NAME", name: "mystore.dcl.eth" });
    late.send({ type: "CONFIRM" });

    expect(late.getSnapshot().matches("review")).toBe(true);
    expect(late.getSnapshot().context.quotaError).toBe(true);
    expect(lateDeploy).not.toHaveBeenCalled();
  });
});

describe("deployWorldMachine \u{2014} minimal-clicks contract", () => {
  const TARGET_EVENTS = 2;

  it(`reaches deploying in <= ${TARGET_EVENTS} events (destination auto-advances)`, () => {
    const paths = getShortestPaths(deployWorldMachine, {
      input: inputFor(okDeploy, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const toDeploying = paths.filter((p) => (p.state.value as string) === "deploying");
    expect(toDeploying.length).toBeGreaterThan(0);
    const minWeight = Math.min(...toDeploying.map((p) => p.weight));
    expect(minWeight).toBeLessThanOrEqual(TARGET_EVENTS);

    const best = toDeploying.reduce((a, b) => (b.weight < a.weight ? b : a));
    const events = best.steps
      .map((s) => s.event.type as string)
      .filter((t) => t !== "xstate.init");
    expect(events).toEqual(["PICK_NAME", "CONFIRM"]);
    expect(events).not.toContain("CHOOSE_WORLDS");
  });
});

describe("deployWorldMachine \u{2014} LAND destination", () => {
  const LAND = { parcels: ["52,-52", "53,-52"], baseParcel: "52,-52" };
  const LAND_RESULT: DeployResult = { jumpUrl: "https://catalyst.example.com/play/?position=52,-52" };

  function landInput(deploy: DeployFn, track: TrackFn) {
    return { ...inputFor(deploy, track), land: LAND };
  }

  it("with land rights the destination waits; CHOOSE_LAND skips name selection and deploys with target land + base parcel", async () => {
    const track = vi.fn();
    const deploy = vi.fn<DeployFn>(async () => LAND_RESULT);
    const actor = createActor(deployWorldMachine, {
      input: landInput(deploy, track),
    }).start();
    expect(actor.getSnapshot().matches("destination")).toBe(true);

    actor.send({ type: "CHOOSE_LAND" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(actor.getSnapshot().context.target).toBe("land");

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("complete"));

    expect(deploy).toHaveBeenCalledTimes(1);
    expect(deploy.mock.calls[0][0]).toMatchObject({ name: "52,-52", target: "land" });

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(DEPLOY_EVENTS.started);
    expect(events).toContain(DEPLOY_EVENTS.completed);
    expect(events).not.toContain(DEPLOY_EVENTS.nameSelected);
    const started = track.mock.calls.find((c) => c[0] === DEPLOY_EVENTS.started);
    expect(started?.[1]).toMatchObject({ target: "land" });
    const completed = track.mock.calls.find((c) => c[0] === DEPLOY_EVENTS.completed);
    expect(completed?.[1]).toMatchObject({ target: "land" });
  });

  it("BACK from a land review returns to the chooser, and CHOOSE_WORLDS still runs the world flow with target world", async () => {
    const track = vi.fn();
    const deploy = vi.fn(okDeploy);
    const actor = createActor(deployWorldMachine, {
      input: landInput(deploy, track),
    }).start();

    actor.send({ type: "CHOOSE_LAND" });
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("destination")).toBe(true);

    actor.send({ type: "CHOOSE_WORLDS" });
    expect(actor.getSnapshot().matches("selectWorld")).toBe(true);
    actor.send({ type: "PICK_NAME", name: "mystore.dcl.eth" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("complete"));
    expect(deploy.mock.calls[0][0]).toMatchObject({
      name: "mystore.dcl.eth",
      target: "world",
    });
  });

  it("over-quota still blocks a land CONFIRM, and hydrating a land review deep link keeps the land target", () => {
    const deploy = vi.fn(okDeploy);
    const actor = createActor(deployWorldMachine, {
      input: { ...inputFor(deploy, () => {}, { files: FILES_OVER_QUOTA }), land: LAND },
    }).start();
    actor.send({ type: "CHOOSE_LAND" });
    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(actor.getSnapshot().context.quotaError).toBe(true);
    expect(deploy).not.toHaveBeenCalled();

    const hydrated = createActor(deployWorldMachine, {
      input: landInput(okDeploy, () => {}),
      snapshot: resolveDeploySnapshot({
        step: "review",
        trackCtx: TRACK_CTX,
        files: FILES,
        land: LAND,
        target: "land",
        deploy: okDeploy,
        track: () => {},
      }),
    }).start();
    expect(hydrated.getSnapshot().matches("review")).toBe(true);
    expect(hydrated.getSnapshot().context.target).toBe("land");
  });
});

describe("deployWorldMachine \u{2014} deploy failure + retry", () => {
  it("deploy error fires ch_deploy_world_failed; RETRY recovers to complete", async () => {
    const track = vi.fn();
    let calls = 0;
    const deploy: DeployFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("worlds deployer unreachable");
      return okDeploy(args);
    };

    const actor = createActor(deployWorldMachine, {
      input: inputFor(deploy, track),
    }).start();

    actor.send({ type: "CHOOSE_WORLDS" });
    actor.send({ type: "PICK_NAME", name: "mystore.dcl.eth" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("worlds deployer unreachable");
    expect(track.mock.calls.map((c) => c[0])).toContain(DEPLOY_EVENTS.failed);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("complete"));
    expect(track.mock.calls.map((c) => c[0])).toContain(DEPLOY_EVENTS.completed);
  });
});
