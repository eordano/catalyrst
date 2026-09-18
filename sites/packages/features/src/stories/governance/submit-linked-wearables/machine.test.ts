import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  submitLwMachine,
  LW_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveLwSnapshot,
  slugToState,
  stateToSlug,
  defaultCreate,
  type CreateFn,
  type TrackFn,
} from "./machine";
import type {
  CreatedProposal,
  IdentityInput,
  CollectionInput,
  TechnicalInput,
} from "@data/lib/catalyst/governance/submit-linked-wearables";

const RESULT: CreatedProposal = {
  id: "00000000-0000-0000-0000-000000000abc",
  type: "linked_wearables",
};

const okCreate: CreateFn = async () => RESULT;

const VALID_IDENTITY: IdentityInput = {
  name: "Meebits Wearables",
  marketplaceLink: "https://opensea.io/collection/meebits",
  links: ["https://meebits.app"],
};
const VALID_COLLECTION: CollectionInput = {
  imagePreviews: ["https://example.com/preview-1.png"],
  nftCollections: "x".repeat(40),
  items: "12",
};
const VALID_TECHNICAL: TechnicalInput = {
  smartContracts: ["0x7Bd29408f11D2bFC23c34f18275bBf23bB716Bc7"],
  managers: ["0x06012c8cf97BEaD5deAe237070F9587f8E7A266d"],
  programmaticallyGenerated: false,
  method: "",
};

function inputFor(create: CreateFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-linked-wearables",
      variant: "wizard",
      experimentKey: "gv_lw_wizard",
    },
    create,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "identity",
  "collection",
  "technical",
  "review",
  "submitting",
  "success",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "FILL_IDENTITY" as const, identity: VALID_IDENTITY },
  { type: "FILL_COLLECTION" as const, collection: VALID_COLLECTION },
  { type: "FILL_TECHNICAL" as const, technical: VALID_TECHNICAL },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("submitLwMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(submitLwMachine.states));
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

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.identity);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("identity");
    }
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("submitLwMachine \u{2014} deep-link hydration", () => {
  it("first step boots from initial; later steps hydrate silently; only real transitions fire telemetry", async () => {
    const track = vi.fn();
    const create = vi.fn(okCreate);
    const input = inputFor(create, track);

    expect(resolveLwSnapshot({ step: "identity", trackCtx: input.trackCtx })).toBeUndefined();

    const submitting = createActor(submitLwMachine, {
      input,
      snapshot: resolveLwSnapshot({ step: "submitting", trackCtx: input.trackCtx, create, track }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const review = createActor(submitLwMachine, {
      input,
      snapshot: resolveLwSnapshot({ step: "review", trackCtx: input.trackCtx, track }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    const technical = createActor(submitLwMachine, {
      input,
      snapshot: resolveLwSnapshot({ step: "technical", trackCtx: input.trackCtx, track }),
    }).start();
    expect(technical.getSnapshot().matches("technical")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    technical.send({ type: "FILL_TECHNICAL", technical: VALID_TECHNICAL });
    expect(technical.getSnapshot().matches("review")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(LW_EVENTS.technicalFilled);
    expect(events).toContain(LW_EVENTS.reviewReached);
  });
});

describe("submitLwMachine \u{2014} model-based path coverage", () => {
  it("every event-reachable path ends in an expected state and review needs all three form steps", () => {
    const paths = getShortestPaths(submitLwMachine, {
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
    for (const s of ["identity", "collection", "technical", "review", "submitting"]) {
      expect(ends.has(s)).toBe(true);
    }

    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toEqual(
      expect.arrayContaining(["FILL_IDENTITY", "FILL_COLLECTION", "FILL_TECHNICAL"]),
    );
  });
});

describe("submitLwMachine \u{2014} telemetry (happy path)", () => {
  it("identity -> collection -> technical -> review -> submit -> success fires the funnel", async () => {
    const track = vi.fn();
    const actor = createActor(submitLwMachine, {
      input: inputFor(okCreate, track),
    }).start();

    actor.send({ type: "FILL_IDENTITY", identity: VALID_IDENTITY });
    expect(actor.getSnapshot().matches("collection")).toBe(true);

    actor.send({ type: "FILL_COLLECTION", collection: VALID_COLLECTION });
    expect(actor.getSnapshot().matches("technical")).toBe(true);

    actor.send({
      type: "FILL_TECHNICAL",
      technical: VALID_TECHNICAL,
      coAuthors: ["0x" + "1".repeat(40)],
    });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        LW_EVENTS.started,
        LW_EVENTS.identityFilled,
        LW_EVENTS.collectionFilled,
        LW_EVENTS.technicalFilled,
        LW_EVENTS.reviewReached,
        LW_EVENTS.submitting,
        LW_EVENTS.submitted,
      ]),
    );
    expect(events.indexOf(LW_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(LW_EVENTS.submitted),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === LW_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_lw_wizard",
      variant: "wizard",
    });
    const submittedCall = track.mock.calls.find((c) => c[0] === LW_EVENTS.submitted);
    expect(submittedCall?.[1]).toMatchObject({ proposal_id: RESULT.id });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
    expect(actor.getSnapshot().context.coAuthors).toEqual(["0x" + "1".repeat(40)]);
  });
});

describe("submitLwMachine \u{2014} validation guardrail", () => {
  it("an invalid identity stays put with gv_lw_validation_error; a programmatic collection needs Method on the technical step", () => {
    const track = vi.fn();
    const actor = createActor(submitLwMachine, {
      input: inputFor(okCreate, track),
    }).start();

    actor.send({
      type: "FILL_IDENTITY",
      identity: { name: "", marketplaceLink: "not-a-url", links: [""] },
    });
    expect(actor.getSnapshot().matches("identity")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(LW_EVENTS.validationError);
    expect(events).not.toContain(LW_EVENTS.started);
    const errs = actor.getSnapshot().context.errors;
    expect(errs.name).toBeTruthy();
    expect(errs.marketplace_link).toBeTruthy();
    expect(errs.links).toBeTruthy();
    const errCall = track.mock.calls.find((c) => c[0] === LW_EVENTS.validationError);
    expect(errCall?.[1]).toMatchObject({ step: "identity" });

    actor.send({ type: "FILL_IDENTITY", identity: VALID_IDENTITY });
    expect(actor.getSnapshot().matches("collection")).toBe(true);
    actor.send({ type: "FILL_COLLECTION", collection: VALID_COLLECTION });
    expect(actor.getSnapshot().matches("technical")).toBe(true);

    actor.send({
      type: "FILL_TECHNICAL",
      technical: { ...VALID_TECHNICAL, programmaticallyGenerated: true, method: "" },
    });
    expect(actor.getSnapshot().matches("technical")).toBe(true);
    expect(actor.getSnapshot().context.errors.method).toBeTruthy();

    actor.send({
      type: "FILL_TECHNICAL",
      technical: { ...VALID_TECHNICAL, programmaticallyGenerated: true, method: "Generated via a trait pipeline." },
    });
    expect(actor.getSnapshot().matches("review")).toBe(true);
  });
});

describe("submitLwMachine \u{2014} submit failure + retry", () => {
  it("submit error fires gv_lw_submit_error and RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const create: CreateFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("governance unreachable");
      return okCreate(args);
    };

    const actor = createActor(submitLwMachine, {
      input: inputFor(create, track),
    }).start();

    actor.send({ type: "FILL_IDENTITY", identity: VALID_IDENTITY });
    actor.send({ type: "FILL_COLLECTION", collection: VALID_COLLECTION });
    actor.send({ type: "FILL_TECHNICAL", technical: VALID_TECHNICAL });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("governance unreachable");
    expect(track.mock.calls.map((c) => c[0])).toContain(LW_EVENTS.submitError);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(track.mock.calls.map((c) => c[0])).toContain(LW_EVENTS.submitted);
  });
});

describe("defaultCreate", () => {
  it("fails closed instead of fabricating a proposal id", async () => {
    await expect(
      defaultCreate({
        draft: {
          identity: VALID_IDENTITY,
          collection: VALID_COLLECTION,
          technical: VALID_TECHNICAL,
          coAuthors: [],
        },
      }),
    ).rejects.toThrow(
      "linked wearables proposal submission unavailable: DAO governance signer not configured",
    );
  });
});
