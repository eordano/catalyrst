
import { describe, expect, test } from "vitest";

import {
  byCategory,
  isRenderableEmote,
  isRenderableWearable,
  isUsableSlotBinding,
  normalizeEmote,
  normalizeSlotBinding,
  normalizeWearable,
  parseOwned,
  parseOwnedEmotes,
  rarityLabel,
  sortLoadout,
} from "./backpack";
import { normalizeCommunity } from "./communitiesSchema";
import {
  eventCoords,
  eventStart,
  formatEventTime,
  hasAttendeeUser,
  isAttending,
  normalizeEvent,
  normalizeEventAttendee,
} from "./events";
import { isUsableNotification, parseNotifications } from "./notifications";
import { categoryForType, humanizeType, unreadCount } from "./notificationsView";
import type { Notification } from "./notificationsView";
import {
  isRenderablePlace,
  isRenderablePlaceCategory,
  normalizePlace,
  normalizePlaceCategory,
  toCategoryView,
  toPlaceDetail,
  toPlaceView,
} from "./places";
import { normalizeProfileEnvelope } from "./profile";
import { field, hasId, isRecord, keepRow, keepRows, listOf } from "./rows";
import type { RowGuard } from "./rows";
import { EmoteSchema, SlotBindingSchema, WearableSchema } from "./schemas/backpack";
import { CommunitySchema } from "./schemas/communities";
import { EventAttendeeSchema, EventSchema } from "./schemas/events";
import { NotificationSchema } from "./schemas/notifications";
import { CategorySchema, PlaceSchema } from "./schemas/places";
import type { AvatarWire, ProfileEnvelopeWire } from "./schemas/profile";

type GuardCase = {
  what: string;
  guard: RowGuard;
  minimal: Record<string, unknown>;
  map: (row: Record<string, unknown>) => unknown;
  edges: Record<string, unknown>[];
  rejects: unknown[];
};

const CASES: GuardCase[] = [
  {
    what: "place",
    guard: isRenderablePlace,
    minimal: { id: "", base_position: "", positions: [], categories: [] },
    map: (row) => {
      const view = toPlaceView(normalizePlace(row as never));
      return toPlaceDetail(view);
    },
    edges: [
      { id: "", base_position: "", positions: [], categories: [] },
      {
        id: "p",
        base_position: "0,0",
        positions: ["0,0"],
        categories: [],
        title: null,
        user_count: null,
      },
    ],
    rejects: [
      null,
      undefined,
      "a place",
      [],
      {},
      { nope: true },
      { id: "p", positions: [], categories: [] },
      { id: "p", base_position: "0,0", categories: [] },
      { id: "p", base_position: "0,0", positions: [] },
      { id: 7, base_position: "0,0", positions: [], categories: [] },
    ],
  },
  {
    what: "place category",
    guard: isRenderablePlaceCategory,
    minimal: { name: "art" },
    map: (row) => toCategoryView(normalizePlaceCategory(row as never)),
    edges: [{ name: "art" }, { name: "art", active: true, count: 0, i18n: { en: null } }],
    rejects: [null, undefined, {}, { name: "" }, { name: 3 }, { i18n: { en: "Art" } }],
  },
  {
    what: "wearable",
    guard: isRenderableWearable,
    minimal: { urn: "u", name: "", rarity: "", category: "", bodyShapes: [] },
    map: (row) => {
      const w = normalizeWearable(row as never);
      byCategory([w]);
      rarityLabel(w.rarity);
      return w;
    },
    edges: [
      { urn: "u", name: "", rarity: "", category: "", bodyShapes: [] },
      {
        urn: "urn:decentraland:off-chain:base-avatars:eyebrows_00",
        name: "Eyebrows",
        rarity: "base",
        category: "eyebrows",
        bodyShapes: ["urn:decentraland:off-chain:base-avatars:BaseFemale"],
        isSmart: false,
      },
    ],
    rejects: [
      null,
      {},
      { name: "orphan" },
      { urn: "", name: "", rarity: "", category: "", bodyShapes: [] },
      { urn: "u", name: "", rarity: "", category: "" },
      { urn: "u", name: "", rarity: 3, category: "", bodyShapes: [] },
    ],
  },
  {
    what: "emote",
    guard: isRenderableEmote,
    minimal: { urn: "u", name: "" },
    map: (row) => normalizeEmote(row as never),
    edges: [
      { urn: "u", name: "" },
      { urn: "u", name: "Wave", category: null, loop: null, rarity: null },
    ],
    rejects: [null, {}, { name: "Nameless" }, { urn: "" }, { urn: "u" }, { urn: 1, name: "x" }],
  },
  {
    what: "slot binding",
    guard: isUsableSlotBinding,
    minimal: { slot: 0, urn: "u" },
    map: (row) => sortLoadout([normalizeSlotBinding(row as never)]),
    edges: [
      { slot: 0, urn: "u" },
      { slot: 9, urn: "u", name: null },
    ],
    rejects: [
      null,
      {},
      { slot: 1 },
      { urn: "u" },
      { slot: "2", urn: "u" },
      { slot: 42, urn: "u" },
      { slot: -1, urn: "u" },
      { slot: 1.5, urn: "u" },
      { slot: 1, urn: "" },
    ],
  },
  {
    what: "community",
    guard: hasId,
    minimal: { id: "" },
    map: (row) => normalizeCommunity(row as never),
    edges: [{ id: "" }, { id: "c", ownerName: null, thumbnailUrl: null }],
    rejects: [null, undefined, {}, [], "c", { name: "no id" }, { id: 4 }],
  },
  {
    what: "event",
    guard: hasId,
    minimal: { id: "" },
    map: (row) => {
      const e = normalizeEvent(row as never);
      return [eventCoords(e), formatEventTime(eventStart(e))];
    },
    edges: [{ id: "" }, { id: "e", x: null, y: null, position: [], coordinates: [] }],
    rejects: [null, {}, { name: "no id" }],
  },
  {
    what: "event attendee",
    guard: hasAttendeeUser,
    minimal: { user: "" },
    map: (row) => {
      const a = normalizeEventAttendee(row as never);
      return [a.user_name, isAttending([a], a.user || "0xa")];
    },
    edges: [
      { user: "" },
      {
        event_id: "e",
        user: "0xabc0000000000000000000000000000000000001",
        user_name: null,
        created_at: "2026-08-01T12:00:00Z",
      },
    ],
    rejects: [null, {}, { user: 7 }, { user_name: "no address" }],
  },
  {
    what: "notification",
    guard: isUsableNotification,
    minimal: { id: "n", type: "t", timestamp: 0 },
    map: (row) => {
      const n = row as unknown as Notification;
      return [categoryForType(n.type), humanizeType(n.type), unreadCount([n])];
    },
    edges: [
      { id: "n", type: "t", timestamp: 0 },
      {
        id: "n",
        type: "item_sold",
        address: "",
        timestamp: -1,
        read: false,
        created_at: "",
        updated_at: "",
        metadata: {},
      },
    ],
    rejects: [
      null,
      {},
      { id: "n", timestamp: 0 },
      { type: "t", timestamp: 0 },
      { id: "n", type: "t" },
      { id: "n", type: "t", timestamp: "2026-07-30T18:10:00Z" },
      { id: "", type: "t", timestamp: 0 },
      { id: "n", type: "", timestamp: 0 },
    ],
  },
];

const SCHEMAS: Record<string, { safeParse: (v: unknown) => { success: boolean } }> = {
  place: PlaceSchema,
  "place category": CategorySchema,
  wearable: WearableSchema,
  emote: EmoteSchema,
  "slot binding": SlotBindingSchema,
  community: CommunitySchema,
  event: EventSchema,
  "event attendee": EventAttendeeSchema,
  notification: NotificationSchema,
};

describe.each(CASES)("the $what guard", (c) => {
  test("accepts every row its schema would have kept", () => {
    for (const row of c.edges) {
      expect(c.guard(row), `guard rejected an edge row: ${JSON.stringify(row)}`).toBe(true);
    }
  });

  test("rejects rows the mapper could not survive or could only fake", () => {
    for (const row of c.rejects) {
      expect(c.guard(row), `guard accepted: ${JSON.stringify(row)}`).toBe(false);
    }
  });

  test("the mapper survives the minimal row the guard admits", () => {
    expect(() => c.map(c.minimal)).not.toThrow();
  });

  test("the minimal row is minimal: dropping any field of it fails the guard", () => {
    for (const key of Object.keys(c.minimal)) {
      const { [key]: _dropped, ...rest } = c.minimal;
      expect(c.guard(rest), `the guard does not actually require ${key}`).toBe(false);
    }
  });
});

describe("the acceptance property, stated once more against the real schemas", () => {
  test("a schema-valid edge row is never rejected by its guard", () => {
    for (const c of CASES) {
      const schema = SCHEMAS[c.what];
      for (const row of c.edges) {
        if (!schema?.safeParse(row).success) continue;
        expect(c.guard(row), `${c.what}: schema kept a row the guard dropped`).toBe(true);
      }
    }
  });

  test("isRenderablePlaceCategory is stricter than its schema, and only it", () => {
    const unnamed = { name: "", active: true, count: 0, i18n: { en: null } };
    expect(CategorySchema.safeParse(unnamed).success).toBe(true);
    expect(isRenderablePlaceCategory(unnamed)).toBe(false);
  });
});

describe("the helpers a guard is built out of", () => {
  test("isRecord admits objects and nothing that would throw on a property read", () => {
    expect(isRecord({})).toBe(true);
    for (const v of [null, undefined, [], "s", 1, true]) expect(isRecord(v)).toBe(false);
  });

  test("field reads through anything a stub may have waved past", () => {
    expect(field({ data: 1 }, "data")).toBe(1);
    expect(field(null, "data")).toBeUndefined();
    expect(field("nope", "data")).toBeUndefined();
    expect(field({ error: "boom" }, "data")).toBeUndefined();
  });

  test("listOf answers an array or an empty one, never undefined", () => {
    expect(listOf([1, 2])).toEqual([1, 2]);
    for (const v of [null, undefined, {}, "ab"]) expect(listOf(v)).toEqual([]);
  });

  test("keepRow applies the guard to what the schema handed back", () => {
    expect(keepRow({ id: "p", extra: 1 }, CommunitySchema, hasId)).toBeNull();
    const ok = { name: "art", active: true, count: 0, i18n: { en: null } };
    expect(keepRow(ok, CategorySchema, isRenderablePlaceCategory)).toMatchObject({ name: "art" });
  });

  test("keepRows drops rather than throws, on rows and on the list itself", () => {
    const rows = [
      { name: "art", active: true, count: 1, i18n: { en: null } },
      { name: "", active: true, count: 1, i18n: { en: null } },
      null,
    ];
    expect(keepRows(rows, CategorySchema, isRenderablePlaceCategory, (r) => r.name)).toEqual([
      "art",
    ]);
    expect(keepRows(undefined, CategorySchema, isRenderablePlaceCategory, (r) => r.name)).toEqual(
      [],
    );
  });
});

describe("the readers whose row semantics the guards changed", () => {
  test("parseOwned keeps urns and nothing that is not one", () => {
    expect(parseOwned([{ urn: "a" }, { urn: 5 }, { name: "b" }, null, "c"])).toEqual(["a"]);
    expect(parseOwnedEmotes([{ urn: "e" }, {}])).toEqual(["e"]);
    expect(parseOwned("not a list")).toEqual([]);
  });

  test("parseNotifications keeps the sort meaningful by dropping non-numeric timestamps", () => {
    const rows = parseNotifications({
      notifications: [
        { id: "a", type: "item_sold", address: "", timestamp: 1, read: false, created_at: "", updated_at: "", metadata: {} },
        { id: "b", type: "item_sold", address: "", timestamp: 9, read: false, created_at: "", updated_at: "", metadata: {} },
        { id: "c", type: "item_sold", address: "", timestamp: "9", read: false, created_at: "", updated_at: "", metadata: {} },
      ],
    });
    expect(rows.map((n) => n.id)).toEqual(["b", "a"]);
  });

  test("parseNotifications reads an envelope it cannot recognise as empty", () => {
    expect(parseNotifications({ oops: 1 })).toEqual([]);
    expect(parseNotifications(null)).toEqual([]);
  });

  test("normalizeProfileEnvelope is total on an envelope that carries no avatars", () => {
    expect(normalizeProfileEnvelope({} as ProfileEnvelopeWire).avatars).toEqual([]);
    expect(
      normalizeProfileEnvelope({ avatars: [null, 3] as unknown as AvatarWire[] }).avatars,
    ).toEqual([]);
  });
});
