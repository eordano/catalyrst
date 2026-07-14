import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import type { AcceptFn, Bid } from "@data/lib/catalyst/marketplace/bids";
import {
  acceptBidMachine,
  ACCEPT_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveAcceptSnapshot,
  simulateApprove,
  slugToState,
  stateToSlug,
  type ApproveFn,
  type TrackFn,
} from "./machine";

const BID: Bid = {
  id: "0xbid",
  bidder: "0x3f5ce5fbfe3e9af3971dd833d26ba9b5c936f0be",
  bidderName: "NeonNomad",
  seller: "0xseller",
  price: "1250000000000000000000",
  priceMana: "1,250",
  status: "open",
  expiresAt: 1782604800000,
  createdAt: 1782345600000,
  contractAddress: "0xa06f",
  tokenId: "104",
  network: "ETHEREUM",
  createdRelative: "2 days ago",
  timeLeft: "6 days",
  asset: {
    id: "0xa06f-104",
    name: "Cyber Ronin Jacket",
    issuedId: 104,
    category: "wearable",
    rarity: "legendary",
    bodyShape: "Unisex",
    isSmart: false,
    network: "ethereum",
    description: "",
    thumbnail: null,
    owner: { address: "0xseller", name: "" },
    collection: { name: "Cyber Ronin", address: "0xa06f" },
    order: null,
  },
};

const okApprove: ApproveFn = async () => {};
const okAccept: AcceptFn = async ({ bid }) => ({ txHash: "0xstub", bidId: bid.id });

function inputFor(approve: ApproveFn, accept: AcceptFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "marketplace-accept-bid",
      variant: "wizard",
      experimentKey: "mk_accept_bid_wizard",
    },
    bid: BID,
    approve,
    accept,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "reviewBid",
  "connectWallet",
  "approveNft",
  "confirmAccept",
  "submitTx",
  "success",
  "rejected",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "ACCEPT" as const },
  { type: "REJECT" as const },
  { type: "CONNECT" as const },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("acceptBidMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(acceptBidMachine.states));
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

  it("slugs match the audit-spec step ids for the linear funnel", () => {
    expect(STATE_TO_SLUG.reviewBid).toBe("review-bid");
    expect(STATE_TO_SLUG.connectWallet).toBe("connect-wallet");
    expect(STATE_TO_SLUG.approveNft).toBe("approve-nft");
    expect(STATE_TO_SLUG.confirmAccept).toBe("confirm-accept");
    expect(STATE_TO_SLUG.submitTx).toBe("submit-tx");
    expect(STATE_TO_SLUG.success).toBe("success");
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.reviewBid);
    expect(slugToState(null)).toBe("reviewBid");
    expect(slugToState(undefined)).toBe("reviewBid");
    expect(slugToState("")).toBe("reviewBid");
    expect(slugToState("nope")).toBe("reviewBid");
    expect(slugToState("approve-nft")).toBe("approveNft");
    expect(slugToState("submit-tx")).toBe("submitTx");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("acceptBidMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveAcceptSnapshot({
      step: "reviewBid",
      trackCtx: inputFor(okApprove, okAccept, () => {}).trackCtx,
      bid: BID,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating approveNft does NOT fire telemetry and does NOT auto-run the actor", async () => {
    const track = vi.fn();
    const approve = vi.fn(okApprove);
    const accept = vi.fn(okAccept);
    const snapshot = resolveAcceptSnapshot({
      step: "approveNft",
      trackCtx: inputFor(approve, accept, track).trackCtx,
      bid: BID,
      approve,
      accept,
      track,
    });
    const actor = createActor(acceptBidMachine, {
      input: inputFor(approve, accept, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("approveNft")).toBe(true);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(approve).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("approveNft")).toBe(true);
  });

  it("hydrating submitTx does NOT auto-submit", async () => {
    const track = vi.fn();
    const accept = vi.fn(okAccept);
    const snapshot = resolveAcceptSnapshot({
      step: "submitTx",
      trackCtx: inputFor(okApprove, accept, track).trackCtx,
      bid: BID,
      accept,
      track,
    });
    const actor = createActor(acceptBidMachine, {
      input: inputFor(okApprove, accept, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitTx")).toBe(true);
    await Promise.resolve();
    expect(accept).not.toHaveBeenCalled();
    expect(track).not.toHaveBeenCalled();
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveAcceptSnapshot({
      step: "confirmAccept",
      trackCtx: inputFor(okApprove, okAccept, track).trackCtx,
      bid: BID,
      track,
    });
    const actor = createActor(acceptBidMachine, {
      input: inputFor(okApprove, okAccept, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("confirmAccept")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "CONFIRM" });
    expect(track.mock.calls.map((c) => c[0])).toContain(ACCEPT_EVENTS.submitted);
  });
});

describe("acceptBidMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(acceptBidMachine, {
      input: inputFor(okApprove, okAccept, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("connectWallet")).toBe(true);
    expect(ends.has("rejected")).toBe(true);
  });

  it("reaching connectWallet passes through ACCEPT (the started event)", () => {
    const paths = getShortestPaths(acceptBidMachine, {
      input: inputFor(okApprove, okAccept, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const connect = paths.find((p) => (p.state.value as string) === "connectWallet");
    expect(connect).toBeDefined();
    const events = connect!.steps.map((s) => s.event.type);
    expect(events).toContain("ACCEPT");
  });
});

describe("acceptBidMachine \u{2014} telemetry events (happy path)", () => {
  it("accept -> connect -> approve -> confirm -> submit -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(acceptBidMachine, {
      input: inputFor(okApprove, okAccept, track),
    }).start();

    actor.send({ type: "ACCEPT" });
    expect(actor.getSnapshot().matches("connectWallet")).toBe(true);

    actor.send({ type: "CONNECT" });
    await waitFor(actor, (s) => s.matches("confirmAccept"));

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(ACCEPT_EVENTS.started);
    expect(events).toContain(ACCEPT_EVENTS.walletConnected);
    expect(events).toContain(ACCEPT_EVENTS.nftApproved);
    expect(events).toContain(ACCEPT_EVENTS.confirmReached);
    expect(events).toContain(ACCEPT_EVENTS.submitted);
    expect(events).toContain(ACCEPT_EVENTS.completed);

    expect(events.indexOf(ACCEPT_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(ACCEPT_EVENTS.submitted),
    );
    expect(events.indexOf(ACCEPT_EVENTS.submitted)).toBeLessThan(
      events.indexOf(ACCEPT_EVENTS.completed),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === ACCEPT_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "mk_accept_bid_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result?.bidId).toBe(BID.id);
  });

  it("reject path fires mk_accept_bid_rejected and never approves", () => {
    const track = vi.fn();
    const approve = vi.fn(okApprove);
    const actor = createActor(acceptBidMachine, {
      input: inputFor(approve, okAccept, track),
    }).start();

    actor.send({ type: "REJECT" });
    expect(actor.getSnapshot().matches("rejected")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(ACCEPT_EVENTS.rejected);
    expect(events).not.toContain(ACCEPT_EVENTS.confirmReached);
    expect(approve).not.toHaveBeenCalled();
  });
});

describe("acceptBidMachine \u{2014} failure + retry", () => {
  it("approval error -> RETRY recovers through to success", async () => {
    const track = vi.fn();
    let approveCalls = 0;
    const approve: ApproveFn = async () => {
      approveCalls += 1;
      if (approveCalls === 1) throw new Error("approval rejected by wallet");
    };

    const actor = createActor(acceptBidMachine, {
      input: inputFor(approve, okAccept, track),
    }).start();

    actor.send({ type: "ACCEPT" });
    actor.send({ type: "CONNECT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("approval rejected by wallet");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("confirmAccept"));
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(ACCEPT_EVENTS.completed);
  });
});

describe("simulateApprove", () => {
  it("resolves without touching the network", async () => {
    await expect(simulateApprove({ bid: BID })).resolves.toBeUndefined();
  });
});
