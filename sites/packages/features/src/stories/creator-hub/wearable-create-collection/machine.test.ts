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

const EXPECTED_STATES = new Set([
  "naming",
  "editingItems",
  "reviewing",
  "submitting",
  "done",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "SUBMIT_NAME" as const, name: "Genesis Threads" },
  { type: "ADD_ITEMS" as const, items: ITEMS },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("create-collection \u{2014} pure helpers", () => {
  it("isValidName enforces 1..32 trimmed chars, publishCost charges standard only, parseCollectionType defaults to standard, simulateMint derives a deterministic id", async () => {
    expect(isValidName("")).toBe(false);
    expect(isValidName("   ")).toBe(false);
    expect(isValidName("My Collection")).toBe(true);
    expect(isValidName("x".repeat(32))).toBe(true);
    expect(isValidName("x".repeat(33))).toBe(false);

    expect(publishCost("standard", 3, 100)).toBe(300);
    expect(publishCost("linked", 3, 100)).toBe(0);
    expect(publishCost("standard", 0, 100)).toBe(0);

    for (const v of [null, undefined, "", "standard", "bogus"]) {
      expect(parseCollectionType(v)).toBe("standard");
    }
    for (const v of ["linked", "third_party", "third-party", " Linked "]) {
      expect(parseCollectionType(v)).toBe("linked");
    }

    const out = await simulateMint({ name: "Genesis Threads", type: "standard", items: ITEMS });
    expect(out.collectionId).toBe("sim-genesis-threads");
    expect(out.contractAddress).toMatch(/^0x0+$/);
  });
});

describe("createCollectionMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, and falls back to the first step (including the retired ?step=type)", () => {
    const machineStates = new Set(Object.keys(createCollectionMachine.states));
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

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.naming);
    for (const bad of [null, undefined, "", "nope", "type"]) {
      expect(slugToState(bad)).toBe("naming");
    }
    expect(slugs).not.toContain("type");
    expect(slugToState("review")).toBe("reviewing");
    expect(slugToState("submit")).toBe("submitting");
    expect(slugToState("done")).toBe("done");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("createCollectionMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot; the items step hydrates with no pre-seeded items; a submit snapshot fires no telemetry and never auto-mints", async () => {
    const trackCtx = inputFor(okMint, () => {}).trackCtx;
    expect(resolveCreateSnapshot({ step: "naming", trackCtx })).toBeUndefined();

    const items = createActor(createCollectionMachine, {
      input: inputFor(okMint, () => {}),
      snapshot: resolveCreateSnapshot({ step: "editingItems", trackCtx }),
    }).start();
    expect(items.getSnapshot().matches("editingItems")).toBe(true);
    expect(items.getSnapshot().context.items).toEqual([]);

    const track = vi.fn();
    const mint = vi.fn(okMint);
    const submitting = createActor(createCollectionMachine, {
      input: inputFor(mint, track),
      snapshot: resolveCreateSnapshot({
        step: "submitting",
        trackCtx,
        mint,
        track,
        seed: { name: "Genesis Threads", type: "standard", items: ITEMS },
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.name).toBe("Genesis Threads");
    expect(submitting.getSnapshot().context.items).toHaveLength(2);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(mint).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
  });

  it("a hydrated review cannot submit without items or without a name (never invents one); with both, SUBMIT fires telemetry", async () => {
    const trackCtx = inputFor(okMint, () => {}).trackCtx;
    const track = vi.fn();
    const mint = vi.fn(okMint);
    const noItems = createActor(createCollectionMachine, {
      input: inputFor(mint, track),
      snapshot: resolveCreateSnapshot({
        step: "reviewing",
        trackCtx,
        mint,
        track,
        seed: { name: "Genesis Threads", type: "standard" },
      }),
    }).start();
    expect(noItems.getSnapshot().matches("reviewing")).toBe(true);
    expect(noItems.getSnapshot().context.items).toEqual([]);
    noItems.send({ type: "SUBMIT" });
    await Promise.resolve();
    expect(noItems.getSnapshot().matches("reviewing")).toBe(true);
    expect(mint).not.toHaveBeenCalled();
    expect(track.mock.calls.map((c) => c[0])).not.toContain(CREATE_COLLECTION_EVENTS.submitted);

    const noName = createActor(createCollectionMachine, {
      input: inputFor(okMint, () => {}),
      snapshot: resolveCreateSnapshot({
        step: "reviewing",
        trackCtx,
        seed: { type: "standard", items: ITEMS },
      }),
    }).start();
    expect(noName.getSnapshot().context.name).toBe("");
    noName.send({ type: "SUBMIT" });
    expect(noName.getSnapshot().matches("reviewing")).toBe(true);

    const readyTrack = vi.fn();
    const ready = createActor(createCollectionMachine, {
      input: inputFor(okMint, readyTrack),
      snapshot: resolveCreateSnapshot({
        step: "reviewing",
        trackCtx,
        track: readyTrack,
        seed: { name: "Genesis Threads", type: "standard", items: ITEMS },
      }),
    }).start();
    expect(ready.getSnapshot().matches("reviewing")).toBe(true);
    expect(readyTrack).not.toHaveBeenCalled();
    ready.send({ type: "SUBMIT" });
    expect(ready.getSnapshot().matches("submitting")).toBe(true);
    expect(readyTrack.mock.calls.map((c) => c[0])).toContain(CREATE_COLLECTION_EVENTS.submitted);
  });
});

describe("createCollectionMachine \u{2014} model-based path coverage", () => {
  it("every event-reachable path ends in an expected state and submitting passes through name, items and SUBMIT (no type step)", () => {
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

    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toEqual(expect.arrayContaining(["SUBMIT_NAME", "ADD_ITEMS", "SUBMIT"]));
    expect(events).not.toContain("SELECT_TYPE");
  });
});

describe("createCollectionMachine \u{2014} telemetry events (happy path)", () => {
  it("an invalid name is rejected silently; then name -> items -> review -> submit -> done fires the full funnel with type defaulting to standard", async () => {
    const track = vi.fn();
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, track),
    }).start();
    expect(actor.getSnapshot().context.type).toBe("standard");

    actor.send({ type: "SUBMIT_NAME", name: "   " });
    expect(actor.getSnapshot().matches("naming")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SUBMIT_NAME", name: "Genesis Threads" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);

    actor.send({ type: "ADD_ITEMS", items: ITEMS });
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        CREATE_COLLECTION_EVENTS.started,
        CREATE_COLLECTION_EVENTS.named,
        CREATE_COLLECTION_EVENTS.itemsAdded,
        CREATE_COLLECTION_EVENTS.reviewReached,
        CREATE_COLLECTION_EVENTS.submitted,
        CREATE_COLLECTION_EVENTS.completed,
      ]),
    );
    expect(events.indexOf(CREATE_COLLECTION_EVENTS.started)).toBeLessThan(
      events.indexOf(CREATE_COLLECTION_EVENTS.submitted),
    );
    expect(events.indexOf(CREATE_COLLECTION_EVENTS.submitted)).toBeLessThan(
      events.indexOf(CREATE_COLLECTION_EVENTS.completed),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === CREATE_COLLECTION_EVENTS.started);
    expect(startedCall?.[1]).toMatchObject({ type: "standard" });

    const submitCall = track.mock.calls.find((c) => c[0] === CREATE_COLLECTION_EVENTS.submitted);
    expect(submitCall?.[1]).toMatchObject({ type: "standard", count: 2, cost_mana: 200 });
    expect(submitCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "cwc_create_collection_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
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

    const submitCall = track.mock.calls.find((c) => c[0] === CREATE_COLLECTION_EVENTS.submitted);
    expect(submitCall?.[1]).toMatchObject({ type: "linked", cost_mana: 0 });
  });
});

describe("createCollectionMachine \u{2014} submit failure + retry", () => {
  it("submit error -> GOTO back to items keeps the draft; a second failure -> RETRY recovers to done", async () => {
    const track = vi.fn();
    let calls = 0;
    const mint: MintFn = async (args) => {
      calls += 1;
      if (calls <= 2) throw new Error("catalyst unreachable");
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

    actor.send({ type: "GOTO", step: "editingItems" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);
    expect(actor.getSnapshot().context.items).toHaveLength(2);

    actor.send({ type: "GOTO", step: "reviewing" });
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CREATE_COLLECTION_EVENTS.completed);
    expect(events.filter((e) => e === CREATE_COLLECTION_EVENTS.submitted).length).toBe(3);
    expect(calls).toBe(3);
  });
});

describe("createCollectionMachine \u{2014} GOTO (browser back/forward sync)", () => {
  it("GOTO naming from items preserves the committed name (Back) and GOTO editingItems needs a committed name (Forward), both silent", () => {
    const track = vi.fn();
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, track),
    }).start();

    actor.send({ type: "GOTO", step: "editingItems" });
    expect(actor.getSnapshot().matches("naming")).toBe(true);

    actor.send({ type: "SUBMIT_NAME", name: "Genesis Threads" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);
    track.mockClear();

    actor.send({ type: "GOTO", step: "naming" });
    expect(actor.getSnapshot().matches("naming")).toBe(true);
    expect(actor.getSnapshot().context.name).toBe("Genesis Threads");
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "GOTO", step: "editingItems" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });

  it("GOTO reviewing needs items, never enters submitting/done/error, and is ignored mid-mint", async () => {
    const actor = createActor(createCollectionMachine, {
      input: inputFor(okMint, () => {}),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "Genesis Threads" });
    actor.send({ type: "GOTO", step: "reviewing" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);
    actor.send({ type: "GOTO", step: "submitting" });
    actor.send({ type: "GOTO", step: "done" });
    actor.send({ type: "GOTO", step: "error" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);

    actor.send({ type: "ADD_ITEMS", items: ITEMS });
    actor.send({ type: "GOTO", step: "editingItems" });
    expect(actor.getSnapshot().matches("editingItems")).toBe(true);
    expect(actor.getSnapshot().context.items).toHaveLength(2);

    actor.send({ type: "GOTO", step: "reviewing" });
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);

    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    actor.send({ type: "GOTO", step: "naming" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);

    await waitFor(actor, (s) => s.matches("done"));
  });
});
