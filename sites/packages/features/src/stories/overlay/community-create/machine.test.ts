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

const TRAVERSAL_EVENTS = [
  { type: "OPEN" as const },
  { type: "GET_NAME" as const },
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("communityMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping slug and falls back to create", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(communityMachine.states)));
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(slugToState("gate")).toBe("gate");
    expect(slugToState("details")).toBe("details");
    expect(slugToState("submit")).toBe("submit");
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.create);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("create");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("communityMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots create without a snapshot, hydrates submit silently, and only real transitions track", async () => {
    const track = vi.fn();
    const create = vi.fn(okCreate);
    const trackCtx = inputFor({}).trackCtx;
    expect(resolveCommunitySnapshot({ step: "create", trackCtx })).toBeUndefined();

    const submit = createActor(communityMachine, {
      input: inputFor({ create, track, draft: validDraft() }),
      snapshot: resolveCommunitySnapshot({
        step: "submit",
        trackCtx,
        create,
        track,
        draft: validDraft(),
      }),
    }).start();
    expect(submit.getSnapshot().matches("submit")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
    expect(submit.getSnapshot().matches("submit")).toBe(true);

    const details = createActor(communityMachine, {
      input: inputFor({ create: okCreate, track, draft: validDraft() }),
      snapshot: resolveCommunitySnapshot({ step: "details", trackCtx, track, draft: validDraft() }),
    }).start();
    expect(details.getSnapshot().matches("details")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    details.send({ type: "NEXT" });
    expect(details.getSnapshot().matches("review")).toBe(true);
    expect(names(track)).toContain(COMMUNITY_EVENTS.reviewReached);
  });
});

describe("communityMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("without a NAME the paths reach gate, profile, review and submit via the gate; with a NAME the gate is skipped", () => {
    const paths = getShortestPaths(communityMachine, {
      input: inputFor({ create: okCreate, hasName: false, draft: validDraft() }),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
    }
    for (const s of ["gate", "profile", "review", "submit"]) expect(ends.has(s)).toBe(true);
    const submit = paths.find((p) => (p.state.value as string) === "submit");
    const events = submit!.steps.map((s) => s.event.type);
    for (const e of ["OPEN", "GET_NAME", "NEXT", "SUBMIT"]) expect(events).toContain(e);

    const named = getShortestPaths(communityMachine, {
      input: inputFor({ create: okCreate, hasName: true, draft: validDraft() }),
      events: TRAVERSAL_EVENTS,
    });
    const namedEnds = new Set(named.map((p) => p.state.value as string));
    expect(namedEnds.has("gate")).toBe(false);
    expect(namedEnds.has("profile")).toBe(true);
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

    const events = names(track);
    for (const e of [
      COMMUNITY_EVENTS.opened,
      COMMUNITY_EVENTS.gateViewed,
      COMMUNITY_EVENTS.gatePassed,
      COMMUNITY_EVENTS.reviewReached,
      COMMUNITY_EVENTS.submitAttempted,
      COMMUNITY_EVENTS.created,
    ]) {
      expect(events).toContain(e);
    }
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

describe("communityMachine \u{2014} NAME gate skip + form guards", () => {
  it("owning a NAME skips the gate (gate_passed{had_name:true}); details NEXT needs a community name and review SUBMIT needs the policy ack", () => {
    const track = vi.fn();
    const actor = createActor(communityMachine, {
      input: inputFor({ create: okCreate, track, hasName: true, draft: emptyDraft() }),
    }).start();
    actor.send({ type: "OPEN" });
    expect(actor.getSnapshot().matches("profile")).toBe(true);
    expect(names(track)).not.toContain(COMMUNITY_EVENTS.gateViewed);
    expect(names(track)).toContain(COMMUNITY_EVENTS.gatePassed);
    const gatePassed = track.mock.calls.find((c) => c[0] === COMMUNITY_EVENTS.gatePassed);
    expect(gatePassed?.[1]).toMatchObject({ had_name: true });

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    actor.send({ type: "EDIT", patch: { name: "My Community" } });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    const draft: CommunityDraft = { ...validDraft(), policyAck: false };
    const unacked = createActor(communityMachine, {
      input: inputFor({ create: okCreate, hasName: true, draft }),
    }).start();
    unacked.send({ type: "OPEN" });
    unacked.send({ type: "NEXT" });
    unacked.send({ type: "NEXT" });
    expect(unacked.getSnapshot().matches("review")).toBe(true);
    unacked.send({ type: "SUBMIT" });
    expect(unacked.getSnapshot().matches("review")).toBe(true);
    unacked.send({ type: "EDIT", patch: { policyAck: true } });
    unacked.send({ type: "SUBMIT" });
    expect(unacked.getSnapshot().matches("submit")).toBe(true);
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
    expect(names(track)).toContain(COMMUNITY_EVENTS.submitFailed);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("done"));
    expect(names(track)).toContain(COMMUNITY_EVENTS.created);
  });
});

describe("simulateCreate", () => {
  it("resolves a deterministic id keyed by name and throws on a missing name (would 400 upstream)", async () => {
    const a = await simulateCreate({ draft: validDraft() });
    const b = await simulateCreate({ draft: validDraft() });
    expect(a.id).toMatch(/^[0-9a-f]{64}$/);
    expect(a.id).toBe(b.id);
    expect(a.name).toBe("Crystal Pavilion");
    await expect(simulateCreate({ draft: emptyDraft() })).rejects.toThrow();
  });
});
