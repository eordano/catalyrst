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

const TRAVERSAL_EVENTS = [
  { type: "ACCEPT" as const },
  { type: "REJECT" as const },
  { type: "CONNECT" as const },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("acceptBidMachine \u{2014} URL ?step slug map", () => {
  it("uses the audit-spec step ids, unique and round-tripping, falling back to review-bid", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(acceptBidMachine.states)));
    expect(STATE_TO_SLUG).toMatchObject({
      reviewBid: "review-bid",
      connectWallet: "connect-wallet",
      approveNft: "approve-nft",
      confirmAccept: "confirm-accept",
      submitTx: "submit-tx",
      success: "success",
    });
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.reviewBid);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("reviewBid");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("acceptBidMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots reviewBid without a snapshot, hydrates approveNft/submitTx silently, and only real transitions track", async () => {
    const track = vi.fn();
    const approve = vi.fn(okApprove);
    const accept = vi.fn(okAccept);
    const trackCtx = inputFor(approve, accept, track).trackCtx;
    expect(resolveAcceptSnapshot({ step: "reviewBid", trackCtx, bid: BID })).toBeUndefined();

    const approving = createActor(acceptBidMachine, {
      input: inputFor(approve, accept, track),
      snapshot: resolveAcceptSnapshot({
        step: "approveNft",
        trackCtx,
        bid: BID,
        approve,
        accept,
        track,
      }),
    }).start();
    expect(approving.getSnapshot().matches("approveNft")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(approve).not.toHaveBeenCalled();
    expect(approving.getSnapshot().matches("approveNft")).toBe(true);

    const submitting = createActor(acceptBidMachine, {
      input: inputFor(okApprove, accept, track),
      snapshot: resolveAcceptSnapshot({ step: "submitTx", trackCtx, bid: BID, accept, track }),
    }).start();
    expect(submitting.getSnapshot().matches("submitTx")).toBe(true);
    await Promise.resolve();
    expect(accept).not.toHaveBeenCalled();
    expect(track).not.toHaveBeenCalled();

    const confirming = createActor(acceptBidMachine, {
      input: inputFor(okApprove, okAccept, track),
      snapshot: resolveAcceptSnapshot({ step: "confirmAccept", trackCtx, bid: BID, track }),
    }).start();
    expect(confirming.getSnapshot().matches("confirmAccept")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    confirming.send({ type: "CONFIRM" });
    expect(names(track)).toContain(ACCEPT_EVENTS.submitted);
  });
});

describe("acceptBidMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("the funnel states are event-reachable and connectWallet needs ACCEPT", () => {
    const paths = getShortestPaths(acceptBidMachine, {
      input: inputFor(okApprove, okAccept, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) ends.add(p.state.value as string);
    expect(ends.has("connectWallet")).toBe(true);
    expect(ends.has("rejected")).toBe(true);
    const connect = paths.find((p) => (p.state.value as string) === "connectWallet");
    expect(connect!.steps.map((s) => s.event.type)).toContain("ACCEPT");
  });
});

describe("acceptBidMachine \u{2014} telemetry events", () => {
  it("accept -> connect -> approve -> confirm -> submit -> success fires the full funnel; REJECT never approves", async () => {
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

    const events = names(track);
    for (const e of [
      ACCEPT_EVENTS.started,
      ACCEPT_EVENTS.walletConnected,
      ACCEPT_EVENTS.nftApproved,
      ACCEPT_EVENTS.confirmReached,
      ACCEPT_EVENTS.submitted,
      ACCEPT_EVENTS.completed,
    ]) {
      expect(events).toContain(e);
    }
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

    const rejectTrack = vi.fn();
    const approve = vi.fn(okApprove);
    const rejecting = createActor(acceptBidMachine, {
      input: inputFor(approve, okAccept, rejectTrack),
    }).start();
    rejecting.send({ type: "REJECT" });
    expect(rejecting.getSnapshot().matches("rejected")).toBe(true);
    expect(names(rejectTrack)).toContain(ACCEPT_EVENTS.rejected);
    expect(names(rejectTrack)).not.toContain(ACCEPT_EVENTS.confirmReached);
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
    expect(names(track)).toContain(ACCEPT_EVENTS.completed);
  });
});

describe("simulateApprove", () => {
  it("resolves without touching the network", async () => {
    await expect(simulateApprove({ bid: BID })).resolves.toBeUndefined();
  });
});
