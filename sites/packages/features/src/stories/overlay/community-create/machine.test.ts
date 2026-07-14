import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  communityMachine,
  COMMUNITY_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  FORM_ORDER,
  resolveCommunitySnapshot,
  simulateCreate,
  slugToState,
  stateToSlug,
  type CreateFn,
  type TrackFn,
} from "./machine";
import {
  emptyDraft,
  type CommunityDraft,
  type CreateResult,
} from "@data/lib/catalyst/overlay/community-create";

const RESULT: CreateResult = {
  id: "deadbeef".repeat(8),
  name: "Crystal Pavilion",
  privacy: "public",
  visibility: "all",
};

const okCreate: CreateFn = async () => RESULT;
const failCreate: CreateFn = async () => {
  throw new Error("catalyst unreachable");
};

function validDraft(): CommunityDraft {
  return {
    ...emptyDraft(),
    name: "Crystal Pavilion",
    description: "A community for builders and explorers.",
    hasThumbnail: true,
    policyAck: true,
  };
}

function inputFor(opts: {
  create?: CreateFn;
  track?: TrackFn;
  hasName?: boolean;
  draft?: CommunityDraft;
}) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "bevy-overlay-community-create",
      variant: "wizard",
      experimentKey: "cl_community_create_wizard",
    },
    create: opts.create,
    track: opts.track,
    hasName: opts.hasName,
    draft: opts.draft,
  };
}

const EXPECTED_STATES = new Set([
  "create",
  "gate",
  "profile",
  "details",
  "review",
  "submit",
  "done",
]);

const TRAVERSAL_EVENTS = [
  { type: "OPEN" as const },
  { type: "GET_NAME" as const },
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.create);
    expect(slugToState(null)).toBe("create");
    expect(slugToState(undefined)).toBe("create");
    expect(slugToState("")).toBe("create");
    expect(slugToState("nope")).toBe("create");
    expect(slugToState("gate")).toBe("gate");
    expect(slugToState("details")).toBe("details");
    expect(slugToState("submit")).toBe("submit");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });

  it("FORM_ORDER lists the editable steps in order", () => {
    expect(FORM_ORDER).toEqual(["profile", "details", "review"]);
  });
});

describe("communityMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveCommunitySnapshot({
      step: "create",
      trackCtx: inputFor({}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-create", async () => {
    const track = vi.fn();
    const create = vi.fn(okCreate);
    const snapshot = resolveCommunitySnapshot({
      step: "submit",
      trackCtx: inputFor({ create, track }).trackCtx,
      create,
      track,
      draft: validDraft(),
    });
    const actor = createActor(communityMachine, {
      input: inputFor({ create, track, draft: validDraft() }),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submit")).toBe(true);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submit")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveCommunitySnapshot({
      step: "details",
      trackCtx: inputFor({ track }).trackCtx,
      track,
      draft: validDraft(),
    });
    const actor = createActor(communityMachine, {
      input: inputFor({ create: okCreate, track, draft: validDraft() }),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(COMMUNITY_EVENTS.reviewReached);
  });
});

describe("communityMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("without a NAME: every event-reachable path ends in an expected state, and the gate is reached", () => {
    const paths = getShortestPaths(communityMachine, {
      input: inputFor({ create: okCreate, hasName: false, draft: validDraft() }),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("gate")).toBe(true);
    expect(ends.has("profile")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("submit")).toBe(true);
  });

  it("with a NAME: opening the panel SKIPS the gate", () => {
    const paths = getShortestPaths(communityMachine, {
      input: inputFor({ create: okCreate, hasName: true, draft: validDraft() }),
      events: TRAVERSAL_EVENTS,
    });
    const ends = new Set(paths.map((p) => p.state.value as string));
    expect(ends.has("gate")).toBe(false);
    expect(ends.has("profile")).toBe(true);
  });

  it("reaching submit passes through GET_NAME (gate) then NEXT*", () => {
    const paths = getShortestPaths(communityMachine, {
      input: inputFor({ create: okCreate, hasName: false, draft: validDraft() }),
      events: TRAVERSAL_EVENTS,
    });
    const submit = paths.find((p) => (p.state.value as string) === "submit");
    expect(submit).toBeDefined();
    const events = submit!.steps.map((s) => s.event.type);
    expect(events).toContain("OPEN");
    expect(events).toContain("GET_NAME");
    expect(events).toContain("NEXT");
    expect(events).toContain("SUBMIT");
  });
});

describe("communityMachine \u{2014} telemetry events (happy path, no NAME)", () => {
  it("open -> gate -> profile -> details -> review -> submit -> done fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(communityMachine, {
      input: inputFor({ create: okCreate, track, hasName: false, draft: validDraft() }),
    }).start();

    actor.send({ type: "OPEN" });
    expect(actor.getSnapshot().matches("gate")).toBe(true);

    actor.send({ type: "GET_NAME" });
    expect(actor.getSnapshot().matches("profile")).toBe(true);

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("details")).toBe(true);

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(COMMUNITY_EVENTS.opened);
    expect(events).toContain(COMMUNITY_EVENTS.gateViewed);
    expect(events).toContain(COMMUNITY_EVENTS.gatePassed);
    expect(events).toContain(COMMUNITY_EVENTS.reviewReached);
    expect(events).toContain(COMMUNITY_EVENTS.submitAttempted);
    expect(events).toContain(COMMUNITY_EVENTS.created);

    expect(events.indexOf(COMMUNITY_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(COMMUNITY_EVENTS.created),
    );

    const openedCall = track.mock.calls.find((c) => c[0] === COMMUNITY_EVENTS.opened);
    expect(openedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "cl_community_create_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);

    const gatePassed = track.mock.calls.find((c) => c[0] === COMMUNITY_EVENTS.gatePassed);
    expect(gatePassed?.[1]).toMatchObject({ had_name: false });
  });
});

describe("communityMachine \u{2014} NAME gate skip (owns a NAME)", () => {
  it("open SKIPS the gate, never firing gate_viewed, and fires gate_passed{had_name:true}", () => {
    const track = vi.fn();
    const actor = createActor(communityMachine, {
      input: inputFor({ create: okCreate, track, hasName: true, draft: validDraft() }),
    }).start();

    actor.send({ type: "OPEN" });
    expect(actor.getSnapshot().matches("profile")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).not.toContain(COMMUNITY_EVENTS.gateViewed);
    expect(events).toContain(COMMUNITY_EVENTS.gatePassed);
    const gatePassed = track.mock.calls.find((c) => c[0] === COMMUNITY_EVENTS.gatePassed);
    expect(gatePassed?.[1]).toMatchObject({ had_name: true });
  });
});

describe("communityMachine \u{2014} guards", () => {
  it("details NEXT is blocked until the COMMUNITY NAME is present", () => {
    const actor = createActor(communityMachine, {
      input: inputFor({ create: okCreate, hasName: true, draft: emptyDraft() }),
    }).start();

    actor.send({ type: "OPEN" });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("details")).toBe(true);

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("details")).toBe(true);

    actor.send({ type: "EDIT", patch: { name: "My Community" } });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
  });

  it("review SUBMIT is blocked until the content-policy is acknowledged", () => {
    const draft: CommunityDraft = { ...validDraft(), policyAck: false };
    const actor = createActor(communityMachine, {
      input: inputFor({ create: okCreate, hasName: true, draft }),
    }).start();

    actor.send({ type: "OPEN" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "EDIT", patch: { policyAck: true } });
    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("submit")).toBe(true);
  });
});

describe("communityMachine \u{2014} submit failure + retry", () => {
  it("submit error -> back to review with the error, re-SUBMIT recovers to done", async () => {
    const track = vi.fn();
    let calls = 0;
    const create: CreateFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("catalyst unreachable");
      return okCreate(args);
    };

    const actor = createActor(communityMachine, {
      input: inputFor({ create, track, hasName: true, draft: validDraft() }),
    }).start();

    actor.send({ type: "OPEN" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("review"));
    expect(actor.getSnapshot().context.error).toBe("catalyst unreachable");

    const failEvents = track.mock.calls.map((c) => c[0]);
    expect(failEvents).toContain(COMMUNITY_EVENTS.submitFailed);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("done"));
    expect(track.mock.calls.map((c) => c[0])).toContain(COMMUNITY_EVENTS.created);
  });
});

describe("simulateCreate", () => {
  it("resolves a deterministic id keyed by name (no network)", async () => {
    const a = await simulateCreate({ draft: validDraft() });
    const b = await simulateCreate({ draft: validDraft() });
    expect(a.id).toMatch(/^[0-9a-f]{64}$/);
    expect(a.id).toBe(b.id);
    expect(a.name).toBe("Crystal Pavilion");
  });

  it("throws on a missing name (would 400 upstream)", async () => {
    await expect(simulateCreate({ draft: emptyDraft() })).rejects.toThrow();
  });
});
