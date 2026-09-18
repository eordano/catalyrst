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

describe("computeFee", () => {
  it("totals itemCount * manaPerItem, rolls up per rarity (highest tier first), defaults the per-item fee, and an empty collection costs nothing", () => {
    const fee = computeFee(COLLECTION.items, 100);
    expect(fee.itemCount).toBe(4);
    expect(fee.manaPerItem).toBe(100);
    expect(fee.totalMana).toBe(400);

    const byRarity = Object.fromEntries(fee.lines.map((l) => [l.rarity, l]));
    expect(byRarity.rare.count).toBe(2);
    expect(byRarity.rare.mana).toBe(200);
    expect(byRarity.legendary.count).toBe(1);
    expect(fee.lines[0].rarity).toBe("legendary");
    expect(fee.lines.reduce((a, l) => a + l.mana, 0)).toBe(fee.totalMana);

    expect(computeFee(COLLECTION.items).manaPerItem).toBe(DEFAULT_MANA_PER_ITEM);

    const empty = computeFee([], 100);
    expect(empty.itemCount).toBe(0);
    expect(empty.totalMana).toBe(0);
    expect(empty.lines).toEqual([]);
  });
});

describe("publishMachine \u{2014} URL ?step slug map", () => {
  it("covers every non-transient state, round-trips uniquely, and falls back to the first step", () => {
    const machineStates = new Set(
      Object.keys(publishMachine.states).filter((s) => s !== "decide"),
    );
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.summary);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("summary");
    }
    for (const same of ["cost", "terms", "pay", "submitted"]) {
      expect(slugToState(same)).toBe(same);
    }
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("publishMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot; a pay snapshot fires no telemetry and never auto-publishes; a real transition after hydration fires", async () => {
    const trackCtx = inputFor({ track: () => {} }).trackCtx;
    expect(
      resolvePublishSnapshot({ step: "summary", collection: COLLECTION, trackCtx }),
    ).toBeUndefined();

    const track = vi.fn();
    const publish = vi.fn(okPublish);
    const pay = createActor(publishMachine, {
      input: inputFor({ publish, track }),
      snapshot: resolvePublishSnapshot({ step: "pay", collection: COLLECTION, trackCtx, publish, track }),
    }).start();
    expect(pay.getSnapshot().matches("pay")).toBe(true);
    expect(pay.getSnapshot().context.fee.totalMana).toBe(400);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(publish).not.toHaveBeenCalled();
    expect(pay.getSnapshot().matches("pay")).toBe(true);

    const terms = createActor(publishMachine, {
      input: inputFor({ track }),
      snapshot: resolvePublishSnapshot({ step: "terms", collection: COLLECTION, trackCtx, track }),
    }).start();
    expect(terms.getSnapshot().matches("terms")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    terms.send({ type: "ACCEPT" });
    expect(terms.getSnapshot().matches("pay")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(PUBLISH_EVENTS.termsAccepted);
  });

  it("an empty collection boots blocked with no funnel telemetry, never hydrates pay/submitted, but hydrates the passive panels (cost/terms/error) with terms -> ACCEPT still blocked", () => {
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

    const trackCtx = inputFor({ track: () => {} }).trackCtx;
    for (const step of ["pay", "submitted"] as const) {
      expect(
        resolvePublishSnapshot({ step, collection: EMPTY_COLLECTION, trackCtx }),
      ).toBeUndefined();
    }
    for (const step of ["cost", "terms", "error"] as const) {
      expect(
        resolvePublishSnapshot({ step, collection: EMPTY_COLLECTION, trackCtx }),
      ).toBeDefined();
    }

    const termsTrack = vi.fn();
    const terms = createActor(publishMachine, {
      input: inputFor({ collection: EMPTY_COLLECTION, track: termsTrack }),
      snapshot: resolvePublishSnapshot({
        step: "terms",
        collection: EMPTY_COLLECTION,
        trackCtx,
        track: termsTrack,
      }),
    }).start();
    expect(terms.getSnapshot().matches("terms")).toBe(true);
    terms.send({ type: "ACCEPT" });
    expect(terms.getSnapshot().matches("blocked")).toBe(true);
    expect(termsTrack.mock.calls.map((c) => c[0])).not.toContain(PUBLISH_EVENTS.termsAccepted);
  });
});

describe("publishMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every path ends in an expected state, pay passes through summary/cost/terms, and an empty collection only ever reaches blocked", () => {
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
    for (const s of ["summary", "cost", "terms", "pay"]) {
      expect(ends.has(s)).toBe(true);
    }

    const pay = paths.find((p) => (p.state.value as string) === "pay");
    expect(pay).toBeDefined();
    const events = pay!.steps.map((s) => s.event.type);
    expect(events.filter((e) => e === "NEXT").length).toBeGreaterThanOrEqual(2);
    expect(events).toContain("ACCEPT");

    const emptyPaths = getShortestPaths(publishMachine, {
      input: inputFor({ collection: EMPTY_COLLECTION, track: () => {} }),
      events: TRAVERSAL_EVENTS,
    });
    expect(emptyPaths.length).toBeGreaterThan(0);
    for (const p of emptyPaths) {
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
    expect(events).toEqual(
      expect.arrayContaining([
        PUBLISH_EVENTS.started,
        PUBLISH_EVENTS.costShown,
        PUBLISH_EVENTS.termsAccepted,
        PUBLISH_EVENTS.feePaid,
        PUBLISH_EVENTS.submitted,
        "bd_publish_fee_paid",
        "bd_publish_submitted",
      ]),
    );

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

    const simulated = await simulatePublish({ collection: COLLECTION, totalMana: 400 });
    expect(simulated.txHash).toMatch(/^0xsim/);
  });
});

describe("publishMachine \u{2014} payment failure + retry", () => {
  it("pay error -> BACK returns to terms without submitting; a second failure -> RETRY recovers to submitted", async () => {
    const track = vi.fn();
    let calls = 0;
    const publish: PublishFn = async (args) => {
      calls += 1;
      if (calls <= 2) throw new Error("MANA approval rejected");
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

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("terms")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).not.toContain(PUBLISH_EVENTS.submitted);

    actor.send({ type: "ACCEPT" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("submitted"));
    expect(track.mock.calls.map((c) => c[0])).toContain(PUBLISH_EVENTS.submitted);
    expect(calls).toBe(3);
  });
});
