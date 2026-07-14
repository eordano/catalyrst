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
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(deployWorldMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.destination);
    expect(slugToState(null)).toBe("destination");
    expect(slugToState(undefined)).toBe("destination");
    expect(slugToState("")).toBe("destination");
    expect(slugToState("nope")).toBe("destination");
    expect(slugToState("select-world")).toBe("selectWorld");
    expect(slugToState("select-empty")).toBe("namesEmpty");
    expect(slugToState("review")).toBe("review");
    expect(slugToState("unavailable")).toBe("unavailable");
    expect(slugToState("deploying")).toBe("deploying");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("deployWorldMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveDeploySnapshot({
      step: "destination",
      trackCtx: inputFor(okDeploy, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating `review` does NOT fire telemetry and does NOT auto-deploy", async () => {
    const track = vi.fn();
    const deploy = vi.fn(okDeploy);
    const snapshot = resolveDeploySnapshot({
      step: "review",
      trackCtx: inputFor(deploy, track).trackCtx,
      files: FILES,
      deploy,
      track,
    });
    const actor = createActor(deployWorldMachine, {
      input: inputFor(deploy, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(deploy).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("review")).toBe(true);
  });

  it("hydrating `deploying` does NOT auto-race forward", async () => {
    const track = vi.fn();
    const deploy = vi.fn(okDeploy);
    const snapshot = resolveDeploySnapshot({
      step: "deploying",
      trackCtx: inputFor(deploy, track).trackCtx,
      deploy,
      track,
    });
    const actor = createActor(deployWorldMachine, {
      input: inputFor(deploy, track),
      snapshot,
    }).start();
    expect(actor.getSnapshot().matches("deploying")).toBe(true);
    await Promise.resolve();
    expect(deploy).not.toHaveBeenCalled();
    expect(track).not.toHaveBeenCalled();
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveDeploySnapshot({
      step: "selectWorld",
      trackCtx: inputFor(okDeploy, track).trackCtx,
      track,
    });
    const actor = createActor(deployWorldMachine, {
      input: inputFor(okDeploy, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("selectWorld")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "PICK_NAME", name: "gallery.dcl.eth" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(DEPLOY_EVENTS.nameSelected);
    expect(events).toContain(DEPLOY_EVENTS.reviewReached);
    expect(actor.getSnapshot().context.name).toBe("gallery.dcl.eth");
  });
});

describe("deployWorldMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state (has-names)", () => {
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
    expect(ends.has("selectWorld")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("deploying")).toBe(true);
  });

  it("the empty-NAMEs branch is reachable when namesEmpty is seeded", () => {
    const paths = getShortestPaths(deployWorldMachine, {
      input: inputFor(okDeploy, () => {}, { namesEmpty: true }),
      events: TRAVERSAL_EVENTS,
    });
    const ends = new Set(paths.map((p) => p.state.value as string));
    expect(ends.has("namesEmpty")).toBe(true);
    expect(ends.has("selectWorld")).toBe(false);
    expect(ends.has("review")).toBe(false);
  });

  it("reaching deploying passes through PICK_NAME, CONFIRM (real deploy wired)", () => {
    const paths = getShortestPaths(deployWorldMachine, {
      input: inputFor(okDeploy, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const deploying = paths.find((p) => (p.state.value as string) === "deploying");
    expect(deploying).toBeDefined();
    const events = deploying!.steps.map((s) => s.event.type);
    expect(events).toContain("PICK_NAME");
    expect(events).toContain("CONFIRM");
  });
});

describe("deployWorldMachine \u{2014} telemetry events (happy path)", () => {
  it("destination -> select -> review -> deploy -> finish -> complete fires the full funnel", async () => {
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

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(DEPLOY_EVENTS.started);
    expect(events).toContain(DEPLOY_EVENTS.destinationSelected);
    expect(events).toContain(DEPLOY_EVENTS.nameSelected);
    expect(events).toContain(DEPLOY_EVENTS.reviewReached);
    expect(events).toContain(DEPLOY_EVENTS.confirmReached);
    expect(events).toContain(DEPLOY_EVENTS.completed);
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

  it("without a real deploy wired, CONFIRM lands on `unavailable` (never deploys)", () => {
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
  });

  it("empty-NAMEs path fires ch_deploy_world_names_empty and does not deploy", () => {
    const track = vi.fn();
    const deploy = vi.fn(okDeploy);
    const actor = createActor(deployWorldMachine, {
      input: inputFor(deploy, track, { namesEmpty: true }),
    }).start();

    actor.send({ type: "CHOOSE_WORLDS" });
    expect(actor.getSnapshot().matches("namesEmpty")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(DEPLOY_EVENTS.started);
    expect(events).toContain(DEPLOY_EVENTS.namesEmpty);
    expect(events).not.toContain(DEPLOY_EVENTS.confirmReached);
    expect(deploy).not.toHaveBeenCalled();
  });

  it("over-quota review fires the quota_exceeded guardrail", () => {
    const track = vi.fn();
    const actor = createActor(deployWorldMachine, {
      input: inputFor(okDeploy, track, { files: FILES_OVER_QUOTA }),
    }).start();

    actor.send({ type: "CHOOSE_WORLDS" });
    actor.send({ type: "PICK_NAME", name: "mystore.dcl.eth" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    const reviewCall = track.mock.calls.find((c) => c[0] === DEPLOY_EVENTS.reviewReached);
    expect(reviewCall?.[1]).toMatchObject({ exceeded: true });
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(DEPLOY_EVENTS.quotaExceeded);
  });
});

describe("deployWorldMachine \u{2014} quota guardrail blocks confirm", () => {
  it("over-quota: CONFIRM stays in review, flags quotaError, and does NOT deploy", () => {
    const track = vi.fn();
    const deploy = vi.fn(okDeploy);
    const actor = createActor(deployWorldMachine, {
      input: inputFor(deploy, track, { files: FILES_OVER_QUOTA }),
    }).start();

    actor.send({ type: "PICK_NAME", name: "mystore.dcl.eth" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "CONFIRM" });

    const snap = actor.getSnapshot();
    expect(snap.matches("review")).toBe(true);
    expect(snap.context.quotaError).toBe(true);
    expect(snap.context.result).toBeUndefined();
    expect(deploy).not.toHaveBeenCalled();

    const quotaCalls = track.mock.calls.filter(
      (c) => c[0] === DEPLOY_EVENTS.quotaExceeded,
    );
    expect(quotaCalls.length).toBeGreaterThanOrEqual(2);
    expect(quotaCalls.some((c) => (c[1] as { phase?: string }).phase === "confirm")).toBe(
      true,
    );
  });

  it("SET_FILES: late-arriving real manifest (folder read after mount) drives the guard", () => {
    const track = vi.fn();
    const deploy = vi.fn(okDeploy);
    const actor = createActor(deployWorldMachine, {
      input: inputFor(deploy, track, { files: [] }),
    }).start();

    actor.send({ type: "SET_FILES", files: FILES_OVER_QUOTA });
    actor.send({ type: "PICK_NAME", name: "mystore.dcl.eth" });
    actor.send({ type: "CONFIRM" });

    const snap = actor.getSnapshot();
    expect(snap.matches("review")).toBe(true);
    expect(snap.context.quotaError).toBe(true);
    expect(deploy).not.toHaveBeenCalled();
  });

  it("within-quota: CONFIRM still proceeds to a real deploy (guardrail doesn't over-block)", async () => {
    const track = vi.fn();
    const deploy = vi.fn(okDeploy);
    const actor = createActor(deployWorldMachine, {
      input: inputFor(deploy, track, { files: FILES }),
    }).start();

    actor.send({ type: "PICK_NAME", name: "mystore.dcl.eth" });
    actor.send({ type: "CONFIRM" });

    await waitFor(actor, (s) => s.matches("complete"));
    expect(deploy).toHaveBeenCalledTimes(1);
    expect(actor.getSnapshot().context.quotaError).toBeUndefined();
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

  it("with land rights the destination step waits instead of auto-advancing", () => {
    const actor = createActor(deployWorldMachine, {
      input: landInput(okDeploy, () => {}),
    }).start();
    expect(actor.getSnapshot().matches("destination")).toBe(true);
  });

  it("CHOOSE_LAND skips name selection, deploys with target land + base parcel", async () => {
    const track = vi.fn();
    const deploy = vi.fn<DeployFn>(async () => LAND_RESULT);
    const actor = createActor(deployWorldMachine, {
      input: landInput(deploy, track),
    }).start();

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

  it("BACK from a land review returns to the destination chooser", () => {
    const actor = createActor(deployWorldMachine, {
      input: landInput(okDeploy, () => {}),
    }).start();
    actor.send({ type: "CHOOSE_LAND" });
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("destination")).toBe(true);
  });

  it("CHOOSE_WORLDS from the chooser still runs the world flow with target world", async () => {
    const track = vi.fn();
    const deploy = vi.fn(okDeploy);
    const actor = createActor(deployWorldMachine, {
      input: landInput(deploy, track),
    }).start();

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

  it("over-quota still blocks a land CONFIRM", () => {
    const deploy = vi.fn(okDeploy);
    const actor = createActor(deployWorldMachine, {
      input: { ...inputFor(deploy, () => {}, { files: FILES_OVER_QUOTA }), land: LAND },
    }).start();
    actor.send({ type: "CHOOSE_LAND" });
    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(actor.getSnapshot().context.quotaError).toBe(true);
    expect(deploy).not.toHaveBeenCalled();
  });

  it("hydrating a land review deep-link keeps the land target", () => {
    const snapshot = resolveDeploySnapshot({
      step: "review",
      trackCtx: TRACK_CTX,
      files: FILES,
      land: LAND,
      target: "land",
      deploy: okDeploy,
      track: () => {},
    });
    const actor = createActor(deployWorldMachine, {
      input: landInput(okDeploy, () => {}),
      snapshot,
    }).start();
    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(actor.getSnapshot().context.target).toBe("land");
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
