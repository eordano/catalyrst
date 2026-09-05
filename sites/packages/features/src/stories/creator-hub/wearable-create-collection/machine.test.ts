import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  createCollectionMachine,
  CREATE_COLLECTION_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveCreateSnapshot,
  parseCollectionType,
  slugToState,
  stateToSlug,
  isValidName,
  publishCost,
  simulateMint,
  type CollectionType,
  type DraftItem,
  type MintFn,
  type MintResult,
  type TrackFn,
} from "./machine";

const RESULT: MintResult = {
  collectionId: "sim-test",
  contractAddress: "0x0000000000000000000000000000000000000000",
};

const ITEMS: DraftItem[] = [
  { id: "u1", name: "holographic-jacket.glb", size: 2_400_000, fileType: "glb" },
  { id: "u2", name: "carbon-sneakers.zip", size: 800_000, fileType: "zip" },
];

const okMint: MintFn = async () => RESULT;

function inputFor(
  mint: MintFn,
  track: TrackFn,
  feePerItem = 100,
  type?: CollectionType,
) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "creator-wearable-create-collection",
      variant: "wizard",
      experimentKey: "cwc_create_collection_wizard",
    },
    feePerItem,
    mint,
    track,
    ...(type ? { type } : {}),
  };
}

describe("create-collection \u{2014} pure helpers", () => {
  it("isValidName enforces 1..32 trimmed chars", () => {
    expect(isValidName("")).toBe(false);
    expect(isValidName("   ")).toBe(false);
    expect(isValidName("My Collection")).toBe(true);
    expect(isValidName("x".repeat(32))).toBe(true);
    expect(isValidName("x".repeat(33))).toBe(false);
  });

  it("publishCost: standard pays feePerItem \u{D7} count, linked pays 0", () => {
    expect(publishCost("standard", 3, 100)).toBe(300);
    expect(publishCost("linked", 3, 100)).toBe(0);
    expect(publishCost("standard", 0, 100)).toBe(0);
  });

  it("parseCollectionType: defaults to standard, explicit third-party spellings map to linked", () => {
    expect(parseCollectionType(null)).toBe("standard");
    expect(parseCollectionType(undefined)).toBe("standard");
    expect(parseCollectionType("")).toBe("standard");
    expect(parseCollectionType("standard")).toBe("standard");
    expect(parseCollectionType("bogus")).toBe("standard");
    expect(parseCollectionType("linked")).toBe("linked");
    expect(parseCollectionType("third_party")).toBe("linked");
    expect(parseCollectionType("third-party")).toBe("linked");
    expect(parseCollectionType(" Linked ")).toBe("linked");
  });
});

const EXPECTED_STATES = new Set([
  "naming",
  "editingItems",
  "reviewing",
  "submitting",
  "done",
  "error",
]);

describe("createCollectionMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(createCollectionMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.naming);
    expect(slugToState(null)).toBe("naming");
    expect(slugToState(undefined)).toBe("naming");
    expect(slugToState("")).toBe("naming");
    expect(slugToState("nope")).toBe("naming");
    expect(slugToState("review")).toBe("reviewing");
    expect(slugToState("submit")).toBe("submitting");
    expect(slugToState("done")).toBe("done");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });

  it("the retired ?step=type deep link falls back to the first step", () => {
    expect(slugToState("type")).toBe("naming");
    expect(Object.values(STATE_TO_SLUG)).not.toContain("type");
  });
});

describe("createCollectionMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveCreateSnapshot({
      step: "naming",
      trackCtx: inputFor(okMint, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("the items step hydrates with NO pre-seeded mock items", () => {
    const snapshot = resolveCreateSnapshot({
      step: "editingItems",
      trackCtx: inputFor(okMint, () => {}).trackCtx,
    });
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, () => {}),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("editingItems")).toBe(true);
    expect(actor.getSnapshot().context.items).toEqual([]);
  });

  it("hydrating the submit step does NOT fire telemetry and does NOT auto-mint", async () => {
    const track = vi.fn();
    const mint = vi.fn(okMint);
    const snapshot = resolveCreateSnapshot({
      step: "submitting",
      trackCtx: inputFor(mint, track).trackCtx,
      mint,
      track,
      seed: { name: "Genesis Threads", type: "standard", items: ITEMS },
    });
    const actor = createActor(createCollectionMachine, {
      input: inputFor(mint, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.name).toBe("Genesis Threads");
    expect(actor.getSnapshot().context.items).toHaveLength(2);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(mint).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("a review snapshot with 0 items cannot submit (guard blocks, no telemetry, no mint)", async () => {
    const track = vi.fn();
    const mint = vi.fn(okMint);
    const snapshot = resolveCreateSnapshot({
      step: "reviewing",
      trackCtx: inputFor(mint, track).trackCtx,
      mint,
      track,
      seed: { name: "Genesis Threads", type: "standard" },
    });
    const actor = createActor(createCollectionMachine, {
      input: inputFor(mint, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
    expect(actor.getSnapshot().context.items).toEqual([]);

    actor.send({ type: "SUBMIT" });
    await Promise.resolve();

    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
    expect(mint).not.toHaveBeenCalled();
    expect(track.mock.calls.map((c) => c[0])).not.toContain(
      CREATE_COLLECTION_EVENTS.submitted,
    );
  });

  it("a review snapshot without a name cannot submit, and never invents one", () => {
    const snapshot = resolveCreateSnapshot({
      step: "reviewing",
      trackCtx: inputFor(okMint, () => {}).trackCtx,
      seed: { type: "standard", items: ITEMS },
    });
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, () => {}),
      snapshot,
    }).start();

    expect(actor.getSnapshot().context.name).toBe("");

    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveCreateSnapshot({
      step: "reviewing",
      trackCtx: inputFor(okMint, track).trackCtx,
      track,
      seed: { name: "Genesis Threads", type: "standard", items: ITEMS },
    });
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(
      CREATE_COLLECTION_EVENTS.submitted,
    );
  });
});

const TRAVERSAL_EVENTS = [
  { type: "SUBMIT_NAME" as const, name: "Genesis Threads" },
  { type: "ADD_ITEMS" as const, items: ITEMS },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("createCollectionMachine \u{2014} model-based path coverage", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(createCollectionMachine, {
      input: inputFor(okMint, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("reviewing")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching submitting passes through name, items and SUBMIT (no type step)", () => {
    const paths = getShortestPaths(createCollectionMachine, {
      input: inputFor(okMint, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("SUBMIT_NAME");
    expect(events).toContain("ADD_ITEMS");
    expect(events).toContain("SUBMIT");
    expect(events).not.toContain("SELECT_TYPE");
  });
});

describe("createCollectionMachine \u{2014} telemetry events (happy path)", () => {
  it("name -> items -> review -> submit -> done fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, track),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "Genesis Threads" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);

    actor.send({ type: "ADD_ITEMS", items: ITEMS });
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CREATE_COLLECTION_EVENTS.started);
    expect(events).toContain(CREATE_COLLECTION_EVENTS.named);
    expect(events).toContain(CREATE_COLLECTION_EVENTS.itemsAdded);
    expect(events).toContain(CREATE_COLLECTION_EVENTS.reviewReached);
    expect(events).toContain(CREATE_COLLECTION_EVENTS.submitted);
    expect(events).toContain(CREATE_COLLECTION_EVENTS.completed);

    expect(events.indexOf(CREATE_COLLECTION_EVENTS.started)).toBeLessThan(
      events.indexOf(CREATE_COLLECTION_EVENTS.submitted),
    );
    expect(events.indexOf(CREATE_COLLECTION_EVENTS.submitted)).toBeLessThan(
      events.indexOf(CREATE_COLLECTION_EVENTS.completed),
    );

    const startedCall = track.mock.calls.find(
      (c) => c[0] === CREATE_COLLECTION_EVENTS.started,
    );
    expect(startedCall?.[1]).toMatchObject({ type: "standard" });

    const submitCall = track.mock.calls.find(
      (c) => c[0] === CREATE_COLLECTION_EVENTS.submitted,
    );
    expect(submitCall?.[1]).toMatchObject({
      type: "standard",
      count: 2,
      cost_mana: 200,
    });
    expect(submitCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "cwc_create_collection_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("an invalid name is rejected (no transition, no telemetry)", () => {
    const track = vi.fn();
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, track),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "   " });
    expect(actor.getSnapshot().matches("naming")).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });

  it("type defaults to standard without any explicit input", () => {
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, () => {}),
    }).start();
    expect(actor.getSnapshot().context.type).toBe("standard");
  });

  it("a linked collection (from the ?type URL param) submits with zero MANA cost", () => {
    const track = vi.fn();
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, track, 100, "linked"),
    }).start();

    expect(actor.getSnapshot().context.type).toBe("linked");

    actor.send({ type: "SUBMIT_NAME", name: "Linked Drop" });
    actor.send({ type: "ADD_ITEMS", items: ITEMS });
    actor.send({ type: "SUBMIT" });

    const submitCall = track.mock.calls.find(
      (c) => c[0] === CREATE_COLLECTION_EVENTS.submitted,
    );
    expect(submitCall?.[1]).toMatchObject({ type: "linked", cost_mana: 0 });
  });
});

describe("createCollectionMachine \u{2014} submit failure + retry", () => {
  it("submit error -> RETRY recovers to done", async () => {
    const track = vi.fn();
    let calls = 0;
    const mint: MintFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("catalyst unreachable");
      return okMint(args);
    };

    const actor = createActor(createCollectionMachine, {
      input: inputFor(mint, track),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "Genesis Threads" });
    actor.send({ type: "ADD_ITEMS", items: ITEMS });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("catalyst unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CREATE_COLLECTION_EVENTS.completed);
    expect(
      events.filter((e) => e === CREATE_COLLECTION_EVENTS.submitted).length,
    ).toBe(2);
  });
});

describe("createCollectionMachine \u{2014} GOTO (browser back/forward sync)", () => {
  it("GOTO naming from items preserves the committed name (browser Back)", () => {
    const track = vi.fn();
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, track),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "Genesis Threads" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);
    track.mockClear();

    actor.send({ type: "GOTO", step: "naming" });
    expect(actor.getSnapshot().matches("naming")).toBe(true);
    expect(actor.getSnapshot().context.name).toBe("Genesis Threads");
    expect(track).not.toHaveBeenCalled();
  });

  it("GOTO editingItems from naming (browser Forward) needs a committed name", () => {
    const track = vi.fn();
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, track),
    }).start();

    actor.send({ type: "GOTO", step: "editingItems" });
    expect(actor.getSnapshot().matches("naming")).toBe(true);

    actor.send({ type: "SUBMIT_NAME", name: "Genesis Threads" });
    actor.send({ type: "GOTO", step: "naming" });
    track.mockClear();

    actor.send({ type: "GOTO", step: "editingItems" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });

  it("GOTO reviewing is blocked without items, allowed with them (re-fires review entry)", () => {
    const track = vi.fn();
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, track),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "Genesis Threads" });
    actor.send({ type: "GOTO", step: "reviewing" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);

    actor.send({ type: "ADD_ITEMS", items: ITEMS });
    actor.send({ type: "GOTO", step: "editingItems" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);
    expect(actor.getSnapshot().context.items).toHaveLength(2);

    actor.send({ type: "GOTO", step: "reviewing" });
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
  });

  it("GOTO never enters submitting/done/error and is ignored mid-mint", async () => {
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, () => {}),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "Genesis Threads" });
    actor.send({ type: "GOTO", step: "submitting" });
    actor.send({ type: "GOTO", step: "done" });
    actor.send({ type: "GOTO", step: "error" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);

    actor.send({ type: "ADD_ITEMS", items: ITEMS });
    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    actor.send({ type: "GOTO", step: "naming" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);

    await waitFor(actor, (s) => s.matches("done"));
  });

  it("GOTO from error goes back to items/name (retry path stays reachable)", async () => {
    const mint: MintFn = async () => {
      throw new Error("catalyst unreachable");
    };
    const actor = createActor(createCollectionMachine, {
      input: inputFor(mint, () => {}),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "Genesis Threads" });
    actor.send({ type: "ADD_ITEMS", items: ITEMS });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "GOTO", step: "editingItems" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);
    expect(actor.getSnapshot().context.items).toHaveLength(2);
  });
});

describe("simulateMint", () => {
  it("resolves a deterministic id from the name (no network)", async () => {
    const out = await simulateMint({
      name: "Genesis Threads",
      type: "standard",
      items: ITEMS,
    });
    expect(out.collectionId).toBe("sim-genesis-threads");
    expect(out.contractAddress).toMatch(/^0x0+$/);
  });
});
