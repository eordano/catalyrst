import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  transferMachine,
  TRANSFER_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveTransferSnapshot,
  slugToState,
  stateToSlug,
  simulateTransfer,
  type TransferFn,
  type TransferResult,
  type TransferTarget,
  type TrackFn,
} from "./machine";

const VALID_ADDR = "0x1d9aa2025b67f0f21d1603ce521bda7869098f8a";
const BAD_ADDR = "not-an-address";

const ASSET: TransferTarget = {
  id: "0xabc-28",
  name: "Zombie Mask",
  category: "wearable",
  rarity: "epic",
  network: "ethereum",
};

const RESULT: TransferResult = { txHash: "0xdeadbeef".padEnd(66, "0") };

const okTransfer: TransferFn = async () => RESULT;

function inputFor(transfer: TransferFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "marketplace-transfer",
      variant: "wizard",
      experimentKey: "mk_transfer_wizard",
    },
    transfer,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "selecting",
  "enteringRecipient",
  "reviewing",
  "confirming",
  "submitting",
  "success",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "SELECT_ASSET" as const, asset: ASSET },
  { type: "SUBMIT_RECIPIENT" as const, recipient: VALID_ADDR },
  { type: "SUBMIT_RECIPIENT" as const, recipient: BAD_ADDR },
  { type: "CONFIRM" as const },
  { type: "APPROVE" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("transferMachine \u{2014} URL ?step slug map", () => {
  it("uses the audit-spec step names, unique and round-tripping, falling back to select-asset", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(transferMachine.states)));
    expect(mapped).toEqual(EXPECTED_STATES);
    expect(STATE_TO_SLUG).toMatchObject({
      selecting: "select-asset",
      enteringRecipient: "enter-recipient",
      reviewing: "review",
      confirming: "confirm-transfer",
      submitting: "submit-tx",
      success: "success",
    });
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.selecting);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("selecting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("transferMachine \u{2014} deep-link hydration", () => {
  it("boots selecting without a snapshot, hydrates submit-tx silently, and only real transitions track", async () => {
    const track = vi.fn();
    const transfer = vi.fn(okTransfer);
    const trackCtx = inputFor(transfer, track).trackCtx;
    expect(resolveTransferSnapshot({ step: "selecting", trackCtx })).toBeUndefined();

    const submitting = createActor(transferMachine, {
      input: inputFor(transfer, track),
      snapshot: resolveTransferSnapshot({
        step: "submitting",
        trackCtx,
        transfer,
        track,
        asset: ASSET,
        recipient: VALID_ADDR,
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.asset?.id).toBe(ASSET.id);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(transfer).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const reviewing = createActor(transferMachine, {
      input: inputFor(okTransfer, track),
      snapshot: resolveTransferSnapshot({
        step: "reviewing",
        trackCtx,
        track,
        asset: ASSET,
        recipient: VALID_ADDR,
      }),
    }).start();
    expect(reviewing.getSnapshot().matches("reviewing")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    reviewing.send({ type: "CONFIRM" });
    expect(reviewing.getSnapshot().matches("confirming")).toBe(true);
    expect(names(track)).toContain(TRANSFER_EVENTS.confirmReached);
  });
});

describe("transferMachine \u{2014} model-based path coverage", () => {
  it("every event-reachable path ends in an expected state and submitting needs the full step chain", () => {
    const paths = getShortestPaths(transferMachine, {
      input: inputFor(okTransfer, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    for (const s of ["enteringRecipient", "reviewing", "confirming", "submitting"]) {
      expect(ends.has(s)).toBe(true);
    }
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    const events = submitting!.steps.map((s) => s.event.type);
    for (const e of ["SELECT_ASSET", "SUBMIT_RECIPIENT", "CONFIRM", "APPROVE"]) {
      expect(events).toContain(e);
    }
  });
});

describe("transferMachine \u{2014} telemetry events (happy path)", () => {
  it("a malformed recipient is rejected in place, then select -> recipient -> review -> confirm -> approve -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(transferMachine, { input: inputFor(okTransfer, track) }).start();

    actor.send({ type: "SELECT_ASSET", asset: ASSET });
    expect(actor.getSnapshot().matches("enteringRecipient")).toBe(true);
    actor.send({ type: "SUBMIT_RECIPIENT", recipient: BAD_ADDR });
    expect(actor.getSnapshot().matches("enteringRecipient")).toBe(true);
    expect(names(track)).toContain(TRANSFER_EVENTS.invalidRecipient);
    expect(names(track)).not.toContain(TRANSFER_EVENTS.recipientEntered);

    actor.send({ type: "SUBMIT_RECIPIENT", recipient: VALID_ADDR });
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    actor.send({ type: "APPROVE" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = names(track);
    for (const e of [
      TRANSFER_EVENTS.assetSelected,
      TRANSFER_EVENTS.started,
      TRANSFER_EVENTS.recipientEntered,
      TRANSFER_EVENTS.reviewed,
      TRANSFER_EVENTS.confirmReached,
      TRANSFER_EVENTS.submitted,
      TRANSFER_EVENTS.completed,
    ]) {
      expect(events).toContain(e);
    }
    expect(events.indexOf(TRANSFER_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(TRANSFER_EVENTS.completed),
    );
    const startedCall = track.mock.calls.find((c) => c[0] === TRANSFER_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "mk_transfer_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("transferMachine \u{2014} submit failure + retry", () => {
  it("submit error -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const transfer: TransferFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("rpc unreachable");
      return okTransfer(args);
    };
    const actor = createActor(transferMachine, { input: inputFor(transfer, track) }).start();

    actor.send({ type: "SELECT_ASSET", asset: ASSET });
    actor.send({ type: "SUBMIT_RECIPIENT", recipient: VALID_ADDR });
    actor.send({ type: "CONFIRM" });
    actor.send({ type: "APPROVE" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("rpc unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(names(track)).toContain(TRANSFER_EVENTS.completed);
  });
});

describe("simulateTransfer", () => {
  it("resolves a deterministic, obviously-fake 0x txHash (no network)", async () => {
    const a = await simulateTransfer({ asset: ASSET, recipient: VALID_ADDR });
    const b = await simulateTransfer({ asset: ASSET, recipient: VALID_ADDR });
    expect(a.txHash).toMatch(/^0x[0-9a-f]{64}$/);
    expect(a.txHash).toBe(b.txHash);
  });
});
