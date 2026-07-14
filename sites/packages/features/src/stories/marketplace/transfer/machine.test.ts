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

describe("transferMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(transferMachine.states));
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

  it("slugs match the audit-spec step names", () => {
    expect(STATE_TO_SLUG.selecting).toBe("select-asset");
    expect(STATE_TO_SLUG.enteringRecipient).toBe("enter-recipient");
    expect(STATE_TO_SLUG.reviewing).toBe("review");
    expect(STATE_TO_SLUG.confirming).toBe("confirm-transfer");
    expect(STATE_TO_SLUG.submitting).toBe("submit-tx");
    expect(STATE_TO_SLUG.success).toBe("success");
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.selecting);
    expect(slugToState(null)).toBe("selecting");
    expect(slugToState(undefined)).toBe("selecting");
    expect(slugToState("")).toBe("selecting");
    expect(slugToState("nope")).toBe("selecting");
    expect(slugToState("enter-recipient")).toBe("enteringRecipient");
    expect(slugToState("review")).toBe("reviewing");
    expect(slugToState("submit-tx")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("transferMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveTransferSnapshot({
      step: "selecting",
      trackCtx: inputFor(okTransfer, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submit-tx does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const transfer = vi.fn(okTransfer);
    const snapshot = resolveTransferSnapshot({
      step: "submitting",
      trackCtx: inputFor(transfer, track).trackCtx,
      transfer,
      track,
      asset: ASSET,
      recipient: VALID_ADDR,
    });
    const actor = createActor(transferMachine, {
      input: inputFor(transfer, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.asset?.id).toBe(ASSET.id);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(transfer).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveTransferSnapshot({
      step: "reviewing",
      trackCtx: inputFor(okTransfer, track).trackCtx,
      track,
      asset: ASSET,
      recipient: VALID_ADDR,
    });
    const actor = createActor(transferMachine, {
      input: inputFor(okTransfer, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(TRANSFER_EVENTS.confirmReached);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "SELECT_ASSET" as const, asset: ASSET },
  { type: "SUBMIT_RECIPIENT" as const, recipient: VALID_ADDR },
  { type: "SUBMIT_RECIPIENT" as const, recipient: BAD_ADDR },
  { type: "CONFIRM" as const },
  { type: "APPROVE" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("transferMachine \u{2014} model-based path coverage", () => {
  it("every event-reachable path ends in an expected state", () => {
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
    expect(ends.has("enteringRecipient")).toBe(true);
    expect(ends.has("reviewing")).toBe(true);
    expect(ends.has("confirming")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching submitting passes through the full step chain", () => {
    const paths = getShortestPaths(transferMachine, {
      input: inputFor(okTransfer, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("SELECT_ASSET");
    expect(events).toContain("SUBMIT_RECIPIENT");
    expect(events).toContain("CONFIRM");
    expect(events).toContain("APPROVE");
  });
});

describe("transferMachine \u{2014} telemetry events (happy path)", () => {
  it("select -> recipient -> review -> confirm -> approve -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(transferMachine, {
      input: inputFor(okTransfer, track),
    }).start();

    actor.send({ type: "SELECT_ASSET", asset: ASSET });
    expect(actor.getSnapshot().matches("enteringRecipient")).toBe(true);

    actor.send({ type: "SUBMIT_RECIPIENT", recipient: VALID_ADDR });
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);

    actor.send({ type: "APPROVE" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(TRANSFER_EVENTS.assetSelected);
    expect(events).toContain(TRANSFER_EVENTS.started);
    expect(events).toContain(TRANSFER_EVENTS.recipientEntered);
    expect(events).toContain(TRANSFER_EVENTS.reviewed);
    expect(events).toContain(TRANSFER_EVENTS.confirmReached);
    expect(events).toContain(TRANSFER_EVENTS.submitted);
    expect(events).toContain(TRANSFER_EVENTS.completed);

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

describe("transferMachine \u{2014} invalid recipient guardrail", () => {
  it("a malformed address stays on enter-recipient and fires the guardrail event", () => {
    const track = vi.fn();
    const actor = createActor(transferMachine, {
      input: inputFor(okTransfer, track),
    }).start();

    actor.send({ type: "SELECT_ASSET", asset: ASSET });
    actor.send({ type: "SUBMIT_RECIPIENT", recipient: BAD_ADDR });

    expect(actor.getSnapshot().matches("enteringRecipient")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(TRANSFER_EVENTS.invalidRecipient);
    expect(events).not.toContain(TRANSFER_EVENTS.recipientEntered);

    actor.send({ type: "SUBMIT_RECIPIENT", recipient: VALID_ADDR });
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
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

    const actor = createActor(transferMachine, {
      input: inputFor(transfer, track),
    }).start();

    actor.send({ type: "SELECT_ASSET", asset: ASSET });
    actor.send({ type: "SUBMIT_RECIPIENT", recipient: VALID_ADDR });
    actor.send({ type: "CONFIRM" });
    actor.send({ type: "APPROVE" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("rpc unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(TRANSFER_EVENTS.completed);
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
