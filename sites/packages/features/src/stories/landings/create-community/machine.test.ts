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

const TRAVERSAL_EVENTS = [
  { type: "SIGN_IN" as const },
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("communityMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping slug and falls back to signinGate", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(communityMachine.states)));
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.signinGate);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("signinGate");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("communityMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots signinGate without a snapshot, hydrates submitting silently, and only real transitions track", async () => {
    const track = vi.fn();
    const create = vi.fn(okCreate);
    const trackCtx = inputFor(create, track).trackCtx;
    expect(resolveCommunitySnapshot({ step: "signinGate", trackCtx })).toBeUndefined();

    const submitting = createActor(communityMachine, {
      input: inputFor(create, track),
      snapshot: resolveCommunitySnapshot({
        step: "submitting",
        trackCtx,
        draft: validDraft(),
        create,
        track,
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const places = createActor(communityMachine, {
      input: inputFor(okCreate, track),
      snapshot: resolveCommunitySnapshot({ step: "places", trackCtx, draft: validDraft(), track }),
    }).start();
    expect(places.getSnapshot().matches("places")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    places.send({ type: "NEXT" });
    expect(places.getSnapshot().matches("review")).toBe(true);
    expect(names(track)).toContain(COMMUNITY_EVENTS.reviewReached);
  });
});

describe("communityMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("event paths reach basics, thumbnail, privacy, places, review and submitting, and basics NEXT is guarded by draft validity", () => {
    const paths = getShortestPaths(communityMachine, {
      input: inputFor(okCreate, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
    }
    for (const s of ["basics", "thumbnail", "privacy", "places", "review", "submitting"]) {
      expect(ends.has(s)).toBe(true);
    }
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    const events = submitting!.steps.map((s) => s.event.type);
    for (const e of ["SIGN_IN", "NEXT", "SUBMIT"]) expect(events).toContain(e);

    const actor = createActor(communityMachine, {
      input: inputFor(okCreate, vi.fn(), emptyDraft()),
    }).start();
    actor.send({ type: "SIGN_IN" });
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
    expect(names(track)).toContain(COMMUNITY_EVENTS.gateViewed);

    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("created"));

    const events = names(track);
    for (const e of [
      COMMUNITY_EVENTS.started,
      COMMUNITY_EVENTS.reviewReached,
      COMMUNITY_EVENTS.submitAttempted,
      COMMUNITY_EVENTS.created,
    ]) {
      expect(events).toContain(e);
    }
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
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("review") && s.context.error !== undefined);
    expect(actor.getSnapshot().context.error).toBe("catalyst unreachable");
    expect(names(track)).toContain(COMMUNITY_EVENTS.submitFailed);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("created"));
    expect(names(track)).toContain(COMMUNITY_EVENTS.created);
  });
});

describe("simulate create", () => {
  it("resolves a 64-hex id echoing privacy/visibility, and rejects an empty draft", async () => {
    const created = await simulateCreate({ draft: validDraft() });
    expect(created.id).toMatch(/^[0-9a-f]{64}$/);
    expect(created.privacy).toBe("public");
    expect(created.visibility).toBe("all");
    await expect(simulateCreateCommunity(emptyDraft())).rejects.toThrow(/required/);
  });
});
