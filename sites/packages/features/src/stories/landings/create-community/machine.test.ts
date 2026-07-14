import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  communityMachine,
  COMMUNITY_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveCommunitySnapshot,
  slugToState,
  stateToSlug,
  simulateCreate,
  type CreateFn,
  type CommunityInput,
  type TrackFn,
} from "./machine";
import {
  emptyDraft,
  simulateCreateCommunity,
  type CommunityDraft,
  type CreateResult,
} from "@data/lib/catalyst/overlay/create-community";

const RESULT: CreateResult = {
  id: "deadbeef".repeat(8),
  name: "Crystal Pavilion",
  privacy: "public",
  visibility: "all",
};

const okCreate: CreateFn = async () => RESULT;

function validDraft(): CommunityDraft {
  return {
    ...emptyDraft(),
    name: "Crystal Pavilion",
    description: "A community for builders, collectors, and explorers.",
  };
}

function inputFor(create: CreateFn, track: TrackFn, draft = validDraft()): CommunityInput {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "landings-create-community",
      variant: "wizard",
      experimentKey: "lp_community_wizard",
    },
    draft,
    create,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "signinGate",
  "basics",
  "thumbnail",
  "privacy",
  "places",
  "review",
  "submitting",
  "created",
]);

const TRAVERSAL_EVENTS = [
  { type: "SIGN_IN" as const },
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

describe("communityMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(communityMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.signinGate);
    expect(slugToState(null)).toBe("signinGate");
    expect(slugToState(undefined)).toBe("signinGate");
    expect(slugToState("")).toBe("signinGate");
    expect(slugToState("nope")).toBe("signinGate");
    expect(slugToState("basics")).toBe("basics");
    expect(slugToState("privacy")).toBe("privacy");
    expect(slugToState("submitting")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("communityMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveCommunitySnapshot({
      step: "signinGate",
      trackCtx: inputFor(okCreate, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-create", async () => {
    const track = vi.fn();
    const create = vi.fn(okCreate);
    const snapshot = resolveCommunitySnapshot({
      step: "submitting",
      trackCtx: inputFor(create, track).trackCtx,
      draft: validDraft(),
      create,
      track,
    });
    const actor = createActor(communityMachine, {
      input: inputFor(create, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveCommunitySnapshot({
      step: "places",
      trackCtx: inputFor(okCreate, track).trackCtx,
      draft: validDraft(),
      track,
    });
    const actor = createActor(communityMachine, {
      input: inputFor(okCreate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("places")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(COMMUNITY_EVENTS.reviewReached);
  });
});

describe("communityMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(communityMachine, {
      input: inputFor(okCreate, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("basics")).toBe(true);
    expect(ends.has("thumbnail")).toBe(true);
    expect(ends.has("privacy")).toBe(true);
    expect(ends.has("places")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching submitting passes through SIGN_IN, the form steps, and SUBMIT", () => {
    const paths = getShortestPaths(communityMachine, {
      input: inputFor(okCreate, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("SIGN_IN");
    expect(events).toContain("NEXT");
    expect(events).toContain("SUBMIT");
  });

  it("basics NEXT is blocked when the draft is invalid (empty name/description)", () => {
    const track = vi.fn();
    const actor = createActor(communityMachine, {
      input: inputFor(okCreate, track, emptyDraft()),
    }).start();

    actor.send({ type: "SIGN_IN" });
    expect(actor.getSnapshot().matches("basics")).toBe(true);

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("basics")).toBe(true);

    actor.send({ type: "EDIT", patch: { name: "My Community" } });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("basics")).toBe(true);

    actor.send({ type: "EDIT", patch: { description: "Hello there." } });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("thumbnail")).toBe(true);
  });
});

describe("communityMachine \u{2014} telemetry events (happy path)", () => {
  it("gate -> basics -> ... -> review -> submit -> created fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(communityMachine, {
      input: inputFor(okCreate, track),
    }).start();

    expect(track.mock.calls.map((c) => c[0])).toContain(COMMUNITY_EVENTS.gateViewed);

    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("created"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(COMMUNITY_EVENTS.started);
    expect(events).toContain(COMMUNITY_EVENTS.reviewReached);
    expect(events).toContain(COMMUNITY_EVENTS.submitAttempted);
    expect(events).toContain(COMMUNITY_EVENTS.created);

    expect(events.indexOf(COMMUNITY_EVENTS.started)).toBeLessThan(
      events.indexOf(COMMUNITY_EVENTS.reviewReached),
    );
    expect(events.indexOf(COMMUNITY_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(COMMUNITY_EVENTS.created),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === COMMUNITY_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "lp_community_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("communityMachine \u{2014} create failure + retry", () => {
  it("create error -> back to review -> SUBMIT again recovers to created", async () => {
    const track = vi.fn();
    let calls = 0;
    const create: CreateFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("catalyst unreachable");
      return okCreate(args);
    };

    const actor = createActor(communityMachine, {
      input: inputFor(create, track),
    }).start();

    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("review") && s.context.error !== undefined);
    expect(actor.getSnapshot().context.error).toBe("catalyst unreachable");

    const failEvents = track.mock.calls.map((c) => c[0]);
    expect(failEvents).toContain(COMMUNITY_EVENTS.submitFailed);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("created"));
    expect(track.mock.calls.map((c) => c[0])).toContain(COMMUNITY_EVENTS.created);
  });
});

describe("simulate create", () => {
  it("resolves a 64-hex id and echoes privacy/visibility (no network)", async () => {
    const created = await simulateCreate({ draft: validDraft() });
    expect(created.id).toMatch(/^[0-9a-f]{64}$/);
    expect(created.privacy).toBe("public");
    expect(created.visibility).toBe("all");
  });

  it("rejects an empty draft (name/description required)", async () => {
    await expect(simulateCreateCommunity(emptyDraft())).rejects.toThrow(/required/);
  });
});
