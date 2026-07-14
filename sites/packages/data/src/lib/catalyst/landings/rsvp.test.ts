import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../client")>();
  return { ...actual, postJSON: vi.fn() };
});

import { postJSON } from "../client";
import { buildRsvpCommit } from "./rsvp";
import type { AuthIdentity } from "../../auth/types";

const mPostJSON = vi.mocked(postJSON);

const IDENTITY: AuthIdentity = {
  signer: "0x4e9c4a2502fdf71e93ed8ed6ca9ddbd891d6f295",
  ephemeral: { address: "0xeph", privateKey: "0xdeadbeef" },
  expiration: "2999-01-01T00:00:00.000Z",
  authChain: [],
};

const EVENT_ID = "b8aa88d2-03ff-4453-825a-3f2e7ac00ecc";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("buildRsvpCommit \u{2014} real events attendee write", () => {
  it("going POSTs to the attendees path and returns the server total", async () => {
    mPostJSON.mockResolvedValue({ ok: true, data: [{}, {}, {}], total: 42 });
    const commit = buildRsvpCommit(IDENTITY);

    const out = await commit({ eventId: EVENT_ID, direction: "going", count: 7 });

    expect(out).toEqual({ count: 42 });
    expect(mPostJSON).toHaveBeenCalledWith(
      `/events/api/events/${EVENT_ID}/attendees`,
      undefined,
      expect.objectContaining({ identity: IDENTITY, method: "POST" }),
    );
  });

  it("cancel DELETEs and falls back to the returned list length when total is absent", async () => {
    mPostJSON.mockResolvedValue({ ok: true, data: [{}, {}] });
    const commit = buildRsvpCommit(IDENTITY);

    const out = await commit({ eventId: EVENT_ID, direction: "cancel", count: 7 });

    expect(out).toEqual({ count: 2 });
    expect(mPostJSON).toHaveBeenCalledWith(
      `/events/api/events/${EVENT_ID}/attendees`,
      undefined,
      expect.objectContaining({ method: "DELETE" }),
    );
  });

  it("fails closed without an identity \u{2014} no network call", async () => {
    const commit = buildRsvpCommit(null);
    await expect(
      commit({ eventId: EVENT_ID, direction: "going", count: 0 }),
    ).rejects.toThrow(/sign in/i);
    expect(mPostJSON).not.toHaveBeenCalled();
  });

  it("fails closed without an event id", async () => {
    const commit = buildRsvpCommit(IDENTITY);
    await expect(
      commit({ eventId: "", direction: "going", count: 0 }),
    ).rejects.toThrow();
    expect(mPostJSON).not.toHaveBeenCalled();
  });
});
