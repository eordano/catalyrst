import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  publishMachine,
  PUBLISH_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  DEFAULT_MANA_PER_ITEM,
  computeFee,
  resolvePublishSnapshot,
  slugToState,
  stateToSlug,
  simulatePublish,
  type PublishCollection,
  type PublishFn,
  type PublishResult,
  type TrackFn,
} from "./machine";

const RESULT: PublishResult = { txHash: "0xsimtest" };

const okPublish: PublishFn = async () => RESULT;

const COLLECTION: PublishCollection = {
  id: "col-1",
  name: "Aurora Streetwear Drop",
  items: [
    { id: "w1", name: "Aurora Bomber", rarity: "legendary", kind: "wearable" },
    { id: "w2", name: "Glacier Beanie", rarity: "rare", kind: "wearable" },
    { id: "w3", name: "Polar Mittens", rarity: "rare", kind: "wearable" },
    { id: "e1", name: "Snow Angel", rarity: "epic", kind: "emote" },
  ],
};

const EMPTY_COLLECTION: PublishCollection = { id: "col-empty", name: "Empty", items: [] };

function inputFor(opts: {
  collection?: PublishCollection;
  publish?: PublishFn;
  track: TrackFn;
}) {
  return {
    collection: opts.collection ?? COLLECTION,
    trackCtx: {
      sid: "sid-abc",
      story: "creator-wearable-publish-collection",
      variant: "wizard",
      experimentKey: "bd_wearable_publish_wizard",
    },
    publish: opts.publish ?? okPublish,
    track: opts.track,
  };
}

describe("computeFee", () => {
  it("totals itemCount * manaPerItem (flat on-chain item fee)", () => {
    const fee = computeFee(COLLECTION.items, 100);
    expect(fee.itemCount).toBe(4);
    expect(fee.manaPerItem).toBe(100);
    expect(fee.totalMana).toBe(400);
  });

  it("rolls items up per rarity tier (highest tier first), sums to the total", () => {
    const fee = computeFee(COLLECTION.items, 100);
    const byRarity = Object.fromEntries(fee.lines.map((l) => [l.rarity, l]));
    expect(byRarity.rare.count).toBe(2);
    expect(byRarity.rare.mana).toBe(200);
    expect(byRarity.legendary.count).toBe(1);
    expect(fee.lines[0].rarity).toBe("legendary");
    expect(fee.lines.reduce((a, l) => a + l.mana, 0)).toBe(fee.totalMana);
  });

  it("defaults the per-item fee to DEFAULT_MANA_PER_ITEM", () => {
    expect(computeFee(COLLECTION.items).manaPerItem).toBe(DEFAULT_MANA_PER_ITEM);
  });

  it("an empty collection costs nothing and has no lines", () => {
    const fee = computeFee([], 100);
    expect(fee.itemCount).toBe(0);
    expect(fee.totalMana).toBe(0);
    expect(fee.lines).toEqual([]);
  });
});

describe("publishMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's non-transient states", () => {
    const machineStates = new Set(
      Object.keys(publishMachine.states).filter((s) => s !== "decide"),
    );
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.summary);
    expect(slugToState(null)).toBe("summary");
    expect(slugToState(undefined)).toBe("summary");
    expect(slugToState("")).toBe("summary");
    expect(slugToState("nope")).toBe("summary");
    expect(slugToState("cost")).toBe("cost");
    expect(slugToState("terms")).toBe("terms");
    expect(slugToState("pay")).toBe("pay");
    expect(slugToState("submitted")).toBe("submitted");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("publishMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolvePublishSnapshot({
      step: "summary",
      collection: COLLECTION,
      trackCtx: inputFor({ track: () => {} }).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-publish", async () => {
    const track = vi.fn();
    const publish = vi.fn(okPublish);
    const snapshot = resolvePublishSnapshot({
      step: "pay",
      collection: COLLECTION,
      trackCtx: inputFor({ track }).trackCtx,
      publish,
      track,
    });
    const actor = createActor(publishMachine, {
      input: inputFor({ publish, track }),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("pay")).toBe(true);
    expect(actor.getSnapshot().context.fee.totalMana).toBe(400);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(publish).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("pay")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolvePublishSnapshot({
      step: "terms",
      collection: COLLECTION,
      trackCtx: inputFor({ track }).trackCtx,
      track,
    });
    const actor = createActor(publishMachine, {
      input: inputFor({ track }),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("terms")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "ACCEPT" });
    expect(actor.getSnapshot().matches("pay")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(PUBLISH_EVENTS.termsAccepted);
  });

  it("an empty collection does not hydrate the side-effecting/terminal steps (pay/submitted boot to decide -> blocked)", () => {
    for (const step of ["pay", "submitted"] as const) {
      const snap = resolvePublishSnapshot({
        step,
        collection: EMPTY_COLLECTION,
        trackCtx: inputFor({ track: () => {} }).trackCtx,
      });
      expect(snap).toBeUndefined();
    }
  });

  it("an empty collection DOES hydrate the passive preview panels (cost/terms/error) so their controls are reachable via ?step=", () => {
    for (const step of ["cost", "terms", "error"] as const) {
      const snap = resolvePublishSnapshot({
        step,
        collection: EMPTY_COLLECTION,
        trackCtx: inputFor({ track: () => {} }).trackCtx,
      });
      expect(snap).toBeDefined();
    }
  });

  it("an empty collection DOES hydrate the item-independent terms step (so the accept-terms control is reachable via ?step=terms)", () => {
    const track = vi.fn();
    const snapshot = resolvePublishSnapshot({
      step: "terms",
      collection: EMPTY_COLLECTION,
      trackCtx: inputFor({ track }).trackCtx,
      track,
    });
    expect(snapshot).toBeDefined();

    const actor = createActor(publishMachine, {
      input: inputFor({ collection: EMPTY_COLLECTION, track }),
      snapshot,
    }).start();
    expect(actor.getSnapshot().matches("terms")).toBe(true);

    actor.send({ type: "ACCEPT" });
    expect(actor.getSnapshot().matches("blocked")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).not.toContain(PUBLISH_EVENTS.termsAccepted);
  });
});

const EXPECTED_STATES = new Set([
  "summary",
  "cost",
  "terms",
  "pay",
  "submitted",
  "error",
  "blocked",
]);

const TRAVERSAL_EVENTS = [
  { type: "NEXT" as const },
  { type: "ACCEPT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("publishMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state (with items)", () => {
    const paths = getShortestPaths(publishMachine, {
      input: inputFor({ track: () => {} }),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("decide")).toBe(false);
    expect(ends.has("summary")).toBe(true);
    expect(ends.has("cost")).toBe(true);
    expect(ends.has("terms")).toBe(true);
    expect(ends.has("pay")).toBe(true);
  });

  it("reaching pay passes through summary, cost and terms", () => {
    const paths = getShortestPaths(publishMachine, {
      input: inputFor({ track: () => {} }),
      events: TRAVERSAL_EVENTS,
    });
    const pay = paths.find((p) => (p.state.value as string) === "pay");
    expect(pay).toBeDefined();
    const events = pay!.steps.map((s) => s.event.type);
    expect(events.filter((e) => e === "NEXT").length).toBeGreaterThanOrEqual(2);
    expect(events).toContain("ACCEPT");
  });

  it("an empty collection routes to blocked and stays there", () => {
    const paths = getShortestPaths(publishMachine, {
      input: inputFor({ collection: EMPTY_COLLECTION, track: () => {} }),
      events: TRAVERSAL_EVENTS,
    });
    for (const p of paths) {
      expect(p.state.value).toBe("blocked");
    }
  });
});

describe("publishMachine \u{2014} telemetry events (happy path)", () => {
  it("summary -> cost -> terms -> pay -> submitted fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(publishMachine, {
      input: inputFor({ track }),
    }).start();

    expect(actor.getSnapshot().matches("summary")).toBe(true);

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("cost")).toBe(true);
    actor.send({ type: "NEXT" });
    actor.send({ type: "ACCEPT" });
    await waitFor(actor, (s) => s.matches("submitted"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(PUBLISH_EVENTS.started);
    expect(events).toContain(PUBLISH_EVENTS.costShown);
    expect(events).toContain(PUBLISH_EVENTS.termsAccepted);
    expect(events).toContain("bd_publish_fee_paid");
    expect(events).toContain("bd_publish_submitted");
    expect(events).toContain(PUBLISH_EVENTS.feePaid);
    expect(events).toContain(PUBLISH_EVENTS.submitted);

    const idx = (e: string) => events.indexOf(e);
    expect(idx(PUBLISH_EVENTS.started)).toBeLessThan(idx(PUBLISH_EVENTS.costShown));
    expect(idx(PUBLISH_EVENTS.costShown)).toBeLessThan(idx(PUBLISH_EVENTS.termsAccepted));
    expect(idx(PUBLISH_EVENTS.termsAccepted)).toBeLessThan(idx(PUBLISH_EVENTS.feePaid));
    expect(idx(PUBLISH_EVENTS.feePaid)).toBeLessThan(idx(PUBLISH_EVENTS.submitted));

    const startedCall = track.mock.calls.find((c) => c[0] === PUBLISH_EVENTS.started);
    expect(startedCall?.[1]).toMatchObject({ id: "col-1", itemCount: 4 });
    const costCall = track.mock.calls.find((c) => c[0] === PUBLISH_EVENTS.costShown);
    expect(costCall?.[1]).toMatchObject({ mana: 400 });

    const paidCall = track.mock.calls.find((c) => c[0] === PUBLISH_EVENTS.feePaid);
    expect(paidCall?.[1]).toMatchObject({ mana: 400, tx_hash: "0xsimtest", simulated: true });
    const submittedCall = track.mock.calls.find((c) => c[0] === PUBLISH_EVENTS.submitted);
    expect(submittedCall?.[1]).toMatchObject({ id: "col-1", itemCount: 4, mana: 400, stub: true });

    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "bd_wearable_publish_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("publishMachine \u{2014} empty/no-items collection", () => {
  it("routes to blocked, never crashes, and fires no funnel telemetry", () => {
    const track = vi.fn();
    const publish = vi.fn(okPublish);
    const actor = createActor(publishMachine, {
      input: inputFor({ collection: EMPTY_COLLECTION, publish, track }),
    }).start();

    expect(actor.getSnapshot().matches("blocked")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).not.toContain(PUBLISH_EVENTS.started);
    expect(events).not.toContain(PUBLISH_EVENTS.costShown);
    expect(publish).not.toHaveBeenCalled();

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("blocked")).toBe(true);
  });
});

describe("publishMachine \u{2014} payment failure + retry", () => {
  it("pay error -> RETRY recovers to submitted", async () => {
    const track = vi.fn();
    let calls = 0;
    const publish: PublishFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("MANA approval rejected");
      return okPublish(args);
    };

    const actor = createActor(publishMachine, {
      input: inputFor({ publish, track }),
    }).start();

    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "ACCEPT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("MANA approval rejected");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("submitted"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(PUBLISH_EVENTS.submitted);
  });

  it("pay error -> BACK returns to terms without submitting", async () => {
    const track = vi.fn();
    const failOnce: PublishFn = async () => {
      throw new Error("user rejected signature");
    };
    const actor = createActor(publishMachine, {
      input: inputFor({ publish: failOnce, track }),
    }).start();

    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "ACCEPT" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("terms")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).not.toContain(PUBLISH_EVENTS.submitted);
  });
});

describe("simulatePublish", () => {
  it("resolves a clearly-fake tx hash (no network)", async () => {
    const r = await simulatePublish({ collection: COLLECTION, totalMana: 400 });
    expect(r.txHash).toMatch(/^0xsim/);
  });
});
