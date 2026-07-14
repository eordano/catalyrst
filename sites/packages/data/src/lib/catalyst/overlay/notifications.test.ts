import { describe, expect, it, vi } from "vitest";

import { loadNotifications } from "./notifications.server";

import fixture from "../../../fixtures/bevy-overlay-notifications.json";
import {
  NOTIFICATION_CATEGORIES,
  categoryForType,
  filterByCategory,
  markRead,
  parseFilter,
  parseNotifications,
  relativeTime,
  unreadCount,
  type Notification,
} from "./notifications";

const ROWS = parseNotifications({ notifications: fixture.notifications });

describe("parseNotifications", () => {
  it("parses the fixture rows and keeps the NotificationItem shape", () => {
    expect(ROWS.length).toBe(fixture.notifications.length);
    const first = ROWS[0];
    expect(typeof first.id).toBe("string");
    expect(typeof first.type).toBe("string");
    expect(typeof first.timestamp).toBe("string");
    expect(typeof first.read).toBe("boolean");
  });

  it("sorts newest-first by timestamp", () => {
    for (let i = 1; i < ROWS.length; i++) {
      expect(Number(ROWS[i - 1].timestamp)).toBeGreaterThanOrEqual(
        Number(ROWS[i].timestamp),
      );
    }
  });

  it("drops rows missing required fields, never throws", () => {
    const full = {
      id: "x",
      type: "badge_granted",
      address: "0xabc",
      timestamp: "1",
      read: false,
      created_at: "2026-06-24T00:00:00Z",
      updated_at: "2026-06-24T00:00:00Z",
      metadata: {},
    };
    const mixed = parseNotifications({
      notifications: [
        full,
        // every NotificationItem field is wire-required: a sparse row is
        // dropped, not defaulted into an unread notification from 1970
        { id: "y", type: "badge_granted", timestamp: "1" },
        { nope: true },
        42,
      ],
    });
    expect(mixed.length).toBe(1);
    expect(mixed[0].id).toBe("x");
  });

  it("tolerates a non-envelope input", () => {
    expect(parseNotifications(null)).toEqual([]);
    expect(parseNotifications({})).toEqual([]);
  });
});

describe("categoryForType", () => {
  it("maps the fixture's types onto a known category", () => {
    for (const n of ROWS) {
      expect(NOTIFICATION_CATEGORIES).toContain(categoryForType(n.type));
    }
  });

  it("maps representative canonical types correctly", () => {
    expect(categoryForType("social_service_friendship_request")).toBe("friends");
    expect(categoryForType("badge_granted")).toBe("badge");
    expect(categoryForType("tip_received")).toBe("gift");
    expect(categoryForType("transfer_received")).toBe("gift");
    expect(categoryForType("community_voice_chat_started")).toBe("community");
    expect(categoryForType("item_sold")).toBe("marketplace");
    expect(categoryForType("credits_goal_completed")).toBe("marketplace");
    expect(categoryForType("rental_started")).toBe("marketplace");
  });

  it("falls back to system for governance/reward/worlds/unknown types", () => {
    expect(categoryForType("governance_proposal_enacted")).toBe("system");
    expect(categoryForType("reward_assignment")).toBe("system");
    expect(categoryForType("worlds_access_restored")).toBe("system");
    expect(categoryForType("some_future_type")).toBe("system");
  });
});

describe("filterByCategory", () => {
  it("returns all rows for an empty / 'all' filter", () => {
    expect(filterByCategory(ROWS, "")).toHaveLength(ROWS.length);
    expect(filterByCategory(ROWS, "all")).toHaveLength(ROWS.length);
  });

  it("returns only rows in the requested category", () => {
    const friends = filterByCategory(ROWS, "friends");
    expect(friends.length).toBeGreaterThan(0);
    for (const n of friends) expect(categoryForType(n.type)).toBe("friends");
  });
});

describe("parseFilter", () => {
  it("accepts the six categories and rejects anything else", () => {
    for (const c of NOTIFICATION_CATEGORIES) expect(parseFilter(c)).toBe(c);
    expect(parseFilter("FRIENDS")).toBe("friends");
    expect(parseFilter("all")).toBe("");
    expect(parseFilter("bogus")).toBe("");
    expect(parseFilter(null)).toBe("");
  });
});

describe("markRead", () => {
  const seed: Notification[] = [
    { id: "a", type: "badge_granted", address: "", timestamp: "3", read: false, created_at: "", updated_at: "", metadata: {} },
    { id: "b", type: "item_sold", address: "", timestamp: "2", read: false, created_at: "", updated_at: "", metadata: {} },
    { id: "c", type: "tip_received", address: "", timestamp: "1", read: true, created_at: "", updated_at: "", metadata: {} },
  ];

  it("marks a single id read, leaving others untouched (pure)", () => {
    const next = markRead(seed, ["a"]);
    expect(next).not.toBe(seed);
    expect(next.find((n) => n.id === "a")?.read).toBe(true);
    expect(next.find((n) => n.id === "b")?.read).toBe(false);
    expect(seed.find((n) => n.id === "a")?.read).toBe(false);
  });

  it("marks all read", () => {
    const next = markRead(seed, "all");
    expect(unreadCount(next)).toBe(0);
  });

  it("is idempotent on already-read rows", () => {
    const next = markRead(seed, ["c"]);
    expect(unreadCount(next)).toBe(unreadCount(seed));
  });
});

describe("relativeTime", () => {
  const now = 1_000_000_000_000;
  it("produces deterministic coarse labels", () => {
    expect(relativeTime(now, now)).toBe("Just Now");
    expect(relativeTime(now - 2 * 60_000, now)).toBe("2m");
    expect(relativeTime(now - 3 * 3_600_000, now)).toBe("3h");
    expect(relativeTime(now - 24 * 3_600_000, now)).toBe("Yesterday");
    expect(relativeTime(now - 3 * 86_400_000, now)).toBe("3d");
  });

  it("clamps future timestamps to Just Now", () => {
    expect(relativeTime(now + 5000, now)).toBe("Just Now");
  });
});

describe("loadNotifications", () => {
  const BASE = "http://notifications.test";
  function jsonResponse(body: unknown, status = 200): Response {
    return new Response(JSON.stringify(body), {
      status,
      headers: { "content-type": "application/json" },
    });
  }

  it("reports the feed as unavailable on a non-2xx, not as an empty inbox", async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse({ error: "nope" }, 500));
    const feed = await loadNotifications({ base: BASE, fetchImpl: fetchImpl as never });
    expect(feed.unavailable).toMatch(/500/);
    expect(feed.notifications).toEqual([]);
  });

  it("reports a body that is not the notifications envelope as unavailable", async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse({ nope: true }));
    const feed = await loadNotifications({ base: BASE, fetchImpl: fetchImpl as never });
    expect(feed.unavailable).toMatch(/notifications array/);
  });

  it("reports an empty feed as read and empty", async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse({ notifications: [] }));
    const feed = await loadNotifications({ base: BASE, fetchImpl: fetchImpl as never });
    expect(feed.unavailable).toBeNull();
    expect(feed.notifications).toEqual([]);
  });

  it("returns the rows it read", async () => {
    const fetchImpl = vi.fn(async (_url: string) =>
      jsonResponse({
        notifications: [
          {
            id: "a",
            type: "badge_granted",
            address: "0xabc",
            timestamp: "2",
            read: false,
            created_at: "2026-06-24T00:00:00Z",
            updated_at: "2026-06-24T00:00:00Z",
            metadata: {},
          },
        ],
      }),
    );
    const feed = await loadNotifications({ base: BASE, fetchImpl: fetchImpl as never });
    expect(feed.unavailable).toBeNull();
    expect(feed.notifications).toHaveLength(1);
    expect(feed.address).toBe("0xabc");
  });
});
