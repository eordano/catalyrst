// The payloads and reader calls the perf-parity gate compares across the two
// build modes.
//
// The requirement being stated here: performance mode (DCL_PERF=1) removes
// VALIDATION and nothing else. For a payload the default build accepts, both
// modes must produce byte-identical reader output. Everything a reader does
// besides deciding whether to reject -- repointing a CDN host, turning an absent
// field into null, filing an unknown emote category under the catch-all -- has to
// run in both, which means it cannot live inside a module the perf build
// replaces with an accepting stub.
//
// Split into three groups, because only the first two can demand equality:
//
//   parity      every row is valid, so validation has nothing to reject and the
//               two outputs must be identical.
//   guarded     one row is malformed in a way the READER's own structural guard
//               rejects (rows.ts). A guard is written without zod and lives
//               beside the mapper it protects, so nothing strips it -- both modes
//               drop the row and the two outputs must be identical. This is the
//               group that proves a guard actually runs in perf mode; without it
//               "the guard survives the stub" is a claim about source code.
//   robustness  one row is malformed in a way only the SCHEMA rejects. The
//               default build drops it and perf keeps it -- that difference IS
//               "what is checked" and is allowed. What is not allowed is losing
//               the VALID rows: a reader that throws because a stub handed a bad
//               row to a view mapper returns nothing at all, which is a crash,
//               not a relaxed check.
//
// Kept out of `src/**/*.test.ts` on purpose: a single vitest process resolves
// one mode (vite.validate.js reads DCL_PERF at config time), so these run twice
// under vitest.perf-parity.config.ts and scripts/check-perf-parity.mts diffs the
// two captures.

import {
  parseCatalog,
  parseLoadout,
  projectRawEmote,
} from "../backpack";
import {
  loadCommunities,
  loadCommunity,
  loadCommunityPosts,
} from "../communitiesSchema";
import { serviceBase } from "../client";
import { fetchEventAttendees, fetchEventCategories, fetchEvents } from "../events";
import { parseNotifications } from "../notifications";
import { fetchCategories, fetchPlace, fetchPlaces } from "../placesSchema";
import { fetchProfile, parseProfileEnvelope } from "../profile";

/**
 * The confirmed ways perf mode changes behaviour today. A case declares what it
 * probes so the runner can report a failure as the divergence it is rather than
 * as an anonymous field diff.
 */
export type Divergence =
  | "federated-cdn"
  | "normalization"
  | "bad-row"
  | "row-guard"
  | "key-strip";

export const DIVERGENCE_LEGEND: Record<Divergence, string> = {
  "federated-cdn":
    "a cdn.decentraland.org thumbnail must be repointed at the federated CDN base " +
    "(schemas/communities.ts:16-22). Stubbed, play.catalyst.example.com loads it from the PROD CDN.",
  normalization:
    "transforms must run in both modes: `nullish().transform((v) => v ?? null)` and the " +
    "emote-category reshape (schemas/backpack.ts:87). Stubbed, the field arrives undefined " +
    "where the type says null.",
  "bad-row":
    "the default build DROPS a malformed row; the accepting stub hands it to the view " +
    "mapper, which throws on a field the row does not have and takes the valid rows with it.",
  "row-guard":
    "a reader's structural guard (rows.ts) must run in both modes: it is what keeps an " +
    "unusable row out of a mapper once the schema is gone. Missing, the two modes disagree " +
    "about a row neither build should have kept.",
  "key-strip":
    "a reader that hands a schema more keys than the shape declares relies on zod stripping " +
    "them; the accepting stub returns them all.",
};

/**
 * A value the DEFAULT build must produce, checked against a literal rather than
 * against the other mode.
 *
 * The differential alone is blind in the one direction that matters: delete a
 * normalization outright and BOTH modes lose it, so they still agree and the
 * gate stays green. Confirmed by probe -- removing the CDN rewrite from
 * `normalizeCommunityThumbnail` left this harness reporting 27/27 identical
 * while pointing community thumbnails back at the prod CDN.
 *
 * An anchor is what a differential cannot be: a statement about the RIGHT
 * answer, not merely a matching one. Only normalizations whose loss is a
 * production incident need one.
 */
export type ParityAnchor = {
  id: string;
  /** Pulled out of the default-mode capture for this case id. */
  select: (output: unknown) => unknown;
  expect: unknown;
  why: string;
};

export type ParityCase = {
  /** Stable across runs -- the runner joins the two captures on it. */
  id: string;
  group: "parity" | "guarded" | "robustness";
  probes: Divergence[];
  note: string;
  run: () => Promise<unknown> | unknown;
};

function jsonFetch(routes: Array<[RegExp, unknown]>): typeof fetch {
  return (async (input: RequestInfo | URL) => {
    const url = String(input);
    for (const [match, body] of routes) {
      if (match.test(url)) {
        return new Response(JSON.stringify(body), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
    }
    return new Response(JSON.stringify({ error: "unrouted", url }), { status: 404 });
  }) as typeof fetch;
}

const COMMUNITY_ID = "e99471aa-31c4-4952-abf6-99905445f43b";
const OTHER_COMMUNITY_ID = "b6f0a2c1-4a55-4a0e-9a19-6c5f2b7d1e33";
const ADDR = "0x6b7c0c4e28d1a1eff3fd0f5fca69ab0b1f5f9c1a";

// A signed read: the service fills in every optional field, and the thumbnail
// still points at the PROD CDN the way the social service writes it.
const communitySigned = {
  id: COMMUNITY_ID,
  name: "Winterfest Crew",
  description: "Ice hockey on the lake, every Friday at 20:00 UTC.",
  ownerAddress: "0x0000000000000000000000000000000000000001",
  ownerName: "PepeGawd",
  thumbnailUrl: `https://cdn.decentraland.org/social/communities/${COMMUNITY_ID}/raw-thumbnail.png`,
  privacy: "public",
  visibility: "all",
  membersCount: 12,
  isLive: true,
  role: "member",
};

// The same service, read anonymously: viewer-relative fields are omitted and the
// thumbnail is the literal sentinel. Both normalize to null.
const communityAnonymous = {
  id: OTHER_COMMUNITY_ID,
  name: "Parkour Guild",
  description: "",
  ownerAddress: "0x0000000000000000000000000000000000000002",
  thumbnailUrl: "N/A",
  privacy: "private",
  membersCount: 3,
  isLive: false,
};

// Every field present, on a host the rewrite must leave alone -- so a fix that
// rewrites unconditionally is not a fix, and so this row is identical in both
// modes and can serve as the robustness group's known-good element.
const communityFederated = {
  ...communityAnonymous,
  id: "3f5a1d90-77b2-4a1c-8a1f-9c2e4b6d8a70",
  name: "Museum District",
  ownerName: "curator.eth",
  thumbnailUrl:
    "https://assets-cdn.decentraland.org/social/communities/3f5a1d90-77b2-4a1c-8a1f-9c2e4b6d8a70/raw-thumbnail.png",
  visibility: "unlisted",
  role: "owner",
};

const memberFull = {
  memberAddress: "0x0000000000000000000000000000000000000003",
  role: "moderator",
  joinedAt: "2026-06-01T12:00:00Z",
  name: "skater",
  profilePictureUrl: "https://assets-cdn.decentraland.org/profiles/skater.png",
  hasClaimedName: true,
};

const memberNoJoinDate = {
  memberAddress: "0x0000000000000000000000000000000000000004",
  role: "member",
  name: "guest-4f21",
  profilePictureUrl: "",
  hasClaimedName: false,
};

const postFull = {
  id: "9d1c0a3e-0e2f-4a1b-9c31-b2f7c9d0e111",
  authorAddress: "0x0000000000000000000000000000000000000003",
  content: "Rink is open.",
  createdAt: "2026-07-30T18:04:11Z",
  likesCount: 4,
  isLikedByUser: false,
  authorName: "skater",
  authorProfilePictureUrl: "",
  authorHasClaimedName: true,
};

// The same post as stored before the service backfilled createdAt: the key is
// absent, not null.
const postNoDate = {
  id: "0a4b8c12-3d5e-4f60-9a71-c2d3e4f50222",
  authorAddress: postFull.authorAddress,
  content: "Bring a spare stick.",
  likesCount: 0,
  isLikedByUser: false,
  authorName: postFull.authorName,
  authorProfilePictureUrl: "",
  authorHasClaimedName: true,
};

const placeFull = {
  id: "9270ae0d-b0f2-4c29-a9b6-77ef136f1d4c",
  title: "Winterfest Hockey 2026",
  description: "Pick-up hockey and a warming hut.",
  image:
    "https://peer-ec1.decentraland.org/content/contents/bafybeidzo4ykewzxinkrher4km724igr6xb2gqm5ostpgv5e3gbnjkwnam",
  owner: "0x0000000000000000000000000000000000000005",
  creator_address: "0x0000000000000000000000000000000000000005",
  contact_name: "PepeGawd",
  base_position: "-150,99",
  positions: ["-150,99", "-150,100"],
  categories: ["game", "sports"],
  user_count: 9,
  user_visits: 1204,
  favorites: 31,
  likes: 27,
  like_rate: 1.0,
  highlighted: false,
  world: false,
  world_name: null,
  updated_at: "2026-07-28T09:12:00Z",
};

// The shape catalyrst-places emits for a quiet unnamed parcel: the optional
// columns are absent rather than null.
const placeSparse = {
  id: "1c8f4d22-5a6b-4c7d-8e9f-0a1b2c3d4e5f",
  base_position: "12,-4",
  positions: ["12,-4"],
  categories: [],
  title: null,
  user_count: null,
  user_visits: 0,
  favorites: 0,
  likes: 0,
  highlighted: false,
  world: false,
};

const eventFull = {
  id: "4b1e7a90-2c3d-4e5f-8a9b-0c1d2e3f4a5b",
  name: "Winterfest Opening",
  image: "https://cdn.decentraland.org/events/winterfest.png",
  image_vertical: "https://cdn.decentraland.org/events/winterfest-v.png",
  description: "Doors at 19:00.",
  start_at: "2026-08-10T19:00:00Z",
  finish_at: "2026-08-10T23:00:00Z",
  next_start_at: "2026-08-10T19:00:00Z",
  all_day: false,
  x: -150,
  y: 99,
  position: [-150, 99],
  coordinates: [-150, 99],
  url: "https://events.decentraland.org/event/?id=4b1e7a90",
  user_name: "PepeGawd",
  scene_name: "Winterfest Hockey 2026",
  estate_name: "Winterfest",
  live: true,
  highlighted: true,
  trending: false,
  recurrent: true,
  total_attendees: 84,
  place_id: placeFull.id,
  world: false,
  server: null,
};

const eventSparse = {
  id: "7c2f8b01-3d4e-5f60-9a1b-2c3d4e5f6a7b",
  all_day: true,
  position: [0, 0],
  coordinates: [0, 0],
  live: false,
  highlighted: false,
  trending: false,
  recurrent: false,
  total_attendees: 0,
  world: true,
};

const wearableFull = {
  urn: "urn:decentraland:matic:collections-v2:0x1234:0",
  name: "Ice Skates",
  thumbnail: "https://catalyst.example/content/contents/bafkreiskates",
  rarity: "rare",
  category: "feet",
  bodyShapes: ["urn:decentraland:off-chain:base-avatars:BaseMale"],
  description: "Sharpened.",
  isSmart: false,
  creator: "0x0000000000000000000000000000000000000005",
  network: "matic",
};

const wearableSparse = {
  urn: "urn:decentraland:off-chain:base-avatars:eyebrows_00",
  name: "Eyebrows",
  rarity: "base",
  category: "eyebrows",
  bodyShapes: ["urn:decentraland:off-chain:base-avatars:BaseFemale"],
  isSmart: false,
};

const notificationRows = [
  {
    id: "n-2",
    type: "events_started",
    address: ADDR,
    timestamp: 1_780_000_200_000,
    read: false,
    created_at: "2026-07-30T18:10:00Z",
    updated_at: "2026-07-30T18:10:00Z",
    metadata: { name: "Winterfest Opening", image: "https://cdn.decentraland.org/e.png" },
  },
  {
    id: "n-1",
    type: "item_sold",
    address: ADDR,
    timestamp: 1_780_000_100_000,
    read: true,
    created_at: "2026-07-30T17:00:00Z",
    updated_at: "2026-07-30T17:30:00Z",
    metadata: { itemName: "Ice Skates", price: "1000000000000000000" },
  },
];

const avatarFull = {
  name: "bevynaut",
  hasClaimedName: true,
  nameColor: { r: 1, g: 0.51, b: 0.38 },
  description: "Skating the lake.",
  links: [{ title: "site", url: "https://example.org" }],
  country: "Argentina",
  ethAddress: ADDR,
  userId: ADDR,
  avatar: {
    wearables: [wearableFull.urn],
    snapshots: { face256: "bafkreiface", body: "bafkreibody" },
  },
};

// A profile that has never been edited: the optional strings are simply absent.
const avatarSparse = {
  ethAddress: "0x0000000000000000000000000000000000000006",
  userId: "0x0000000000000000000000000000000000000006",
  avatar: { snapshots: {} },
};

export const CASES: ParityCase[] = [
  {
    id: "communities/loadCommunities",
    group: "parity",
    probes: ["federated-cdn", "normalization"],
    note:
      "a list holding a PROD-CDN thumbnail, the literal N/A sentinel, an already-federated " +
      "URL, and an anonymous row whose viewer-relative fields are absent",
    run: () =>
      loadCommunities(
        {},
        {
          fetchImpl: jsonFetch([
            [
              /\/v1\/communities(\?|$)/,
              {
                data: {
                  results: [communitySigned, communityAnonymous, communityFederated],
                  total: 3,
                },
              },
            ],
          ]),
        },
      ),
  },
  {
    id: "communities/loadCommunity",
    group: "parity",
    probes: ["federated-cdn", "normalization", "key-strip"],
    note:
      "the detail read, which merges members and events into the node before parsing it and " +
      "so relies on the schema to strip them back out",
    run: () =>
      loadCommunity(COMMUNITY_ID, {
        fetchImpl: jsonFetch([
          [/\/members(\?|$)/, { data: { results: [memberFull, memberNoJoinDate] } }],
          [/\/v1\/communities\/[^/?]+(\?|$)/, { data: communitySigned }],
        ]),
      }),
  },
  {
    id: "communities/loadCommunityPosts",
    group: "parity",
    probes: ["normalization"],
    note: "a post whose createdAt the service omitted",
    run: () =>
      loadCommunityPosts(COMMUNITY_ID, {
        fetchImpl: jsonFetch([[/\/posts(\?|$)/, { data: { posts: [postFull, postNoDate] } }]]),
      }),
  },
  {
    id: "places/fetchPlaces",
    group: "parity",
    probes: ["normalization"],
    note: "a populated place and a quiet parcel whose optional columns are absent",
    run: () =>
      fetchPlaces(
        { limit: 48 },
        {
          fetchImpl: jsonFetch([
            [/\/api\/places(\?|$)/, { ok: true, data: [placeFull, placeSparse], total: 2 }],
          ]),
        },
      ),
  },
  {
    id: "places/fetchPlace",
    group: "parity",
    probes: ["normalization"],
    note: "the single-place read of the same sparse row",
    run: () =>
      fetchPlace(placeSparse.id, {
        fetchImpl: jsonFetch([[/\/api\/places\//, { ok: true, data: placeSparse }]]),
      }),
  },
  {
    id: "places/fetchCategories",
    group: "parity",
    probes: ["normalization"],
    note: "a category with an English label and one without",
    run: () =>
      fetchCategories({
        fetchImpl: jsonFetch([
          [
            /\/api\/categories(\?|$)/,
            {
              ok: true,
              data: [
                { name: "game", active: true, count: 120, i18n: { en: "Games" } },
                { name: "art", active: true, count: 44, i18n: {} },
              ],
            },
          ],
        ]),
      }),
  },
  {
    id: "events/fetchEvents",
    group: "parity",
    probes: ["normalization"],
    note: "a full event and one carrying only the fields catalyrst-events always serializes",
    run: () =>
      fetchEvents(
        { limit: 10 },
        {
          fetchImpl: jsonFetch([
            [/\/api\/events(\?|$)/, { data: [eventFull, eventSparse], total: 2 }],
          ]),
        },
      ),
  },
  {
    id: "events/fetchEventAttendees",
    group: "parity",
    probes: ["normalization"],
    note: "an attendee with a display name and one whose user_name is absent",
    run: () =>
      fetchEventAttendees("ev-1", {
        fetchImpl: jsonFetch([
          [
            /\/api\/events\/[^/]+\/attendees/,
            {
              ok: true,
              data: [
                {
                  event_id: "ev-1",
                  user: "0xAbC0000000000000000000000000000000000001",
                  user_name: "Tester",
                  created_at: "2026-08-01T12:00:00Z",
                },
                {
                  event_id: "ev-1",
                  user: "0xabc0000000000000000000000000000000000002",
                  created_at: "2026-08-01T13:00:00Z",
                },
              ],
            },
          ],
        ]),
      }),
  },
  {
    id: "events/fetchEventCategories",
    group: "parity",
    probes: ["normalization"],
    note: "an event category whose i18n.en is absent",
    run: () =>
      fetchEventCategories({
        fetchImpl: jsonFetch([
          [
            /\/api\/events\/categories/,
            {
              data: [
                { name: "music", active: true, i18n: { en: "Music" } },
                { name: "art", active: true, i18n: {} },
              ],
            },
          ],
        ]),
      }),
  },
  {
    id: "backpack/parseCatalog",
    group: "parity",
    probes: ["normalization"],
    note: "a marketplace wearable and a base wearable with no description, creator or network",
    run: () => parseCatalog([wearableFull, wearableSparse]),
  },
  {
    id: "backpack/projectRawEmote",
    group: "parity",
    probes: ["normalization"],
    note:
      "an emote whose metadata states a category the UI has no tab for \u{2014} it belongs in the " +
      "catch-all bucket, not in a tab that does not exist",
    run: () =>
      projectRawEmote({
        urn: "urn:decentraland:matic:collections-v2:0x9999:3",
        name: "Sword Flourish",
        description: "Three-hit combo.",
        thumbnail: "https://catalyst.example/content/contents/bafkreiflourish",
        rarity: "epic",
        emoteDataADR74: { category: "combat", loop: false },
      }),
  },
  {
    id: "backpack/parseLoadout",
    group: "parity",
    probes: ["normalization"],
    note: "an emote-wheel binding the profile stored without a name",
    run: () =>
      parseLoadout([
        { slot: 1, urn: "urn:decentraland:off-chain:base-emotes:wave", name: "Wave" },
        { slot: 2, urn: "urn:decentraland:off-chain:base-emotes:dance" },
      ]),
  },
  {
    id: "profile/parseProfileEnvelope",
    group: "parity",
    probes: ["normalization"],
    note: "an edited profile and one that has never been edited",
    run: () => parseProfileEnvelope({ avatars: [avatarFull, avatarSparse], timestamp: 1_780_000_000 }),
  },
  {
    id: "profile/fetchProfile",
    group: "parity",
    probes: ["normalization"],
    note: "the fetch path, returning the never-edited avatar",
    run: () =>
      fetchProfile(avatarSparse.ethAddress, {
        fetchImpl: jsonFetch([[/\/lambdas\/profile\//, { avatars: [avatarSparse] }]]),
      }),
  },
  {
    id: "notifications/parseNotifications",
    group: "parity",
    probes: [],
    note:
      "the control: this module has no transforms, so it is already parity \u{2014} a failure here " +
      "means the harness itself is comparing the wrong things",
    run: () => parseNotifications({ notifications: notificationRows }),
  },

  {
    id: "places/fetchPlaces+guardedRow",
    group: "guarded",
    probes: ["row-guard", "bad-row"],
    note:
      "a list where one row is not a place at all. `isRenderablePlace` rejects it in both " +
      "modes, so both return the two real parcels \u{2014} and the perf build never reaches " +
      "`positions.length` on a row that has no positions",
    run: () =>
      fetchPlaces(
        {},
        {
          fetchImpl: jsonFetch([
            [
              /\/api\/places(\?|$)/,
              { ok: true, data: [placeFull, { nope: true }, placeSparse], total: 3 },
            ],
          ]),
        },
      ),
  },
  {
    id: "places/fetchPlaces+noCoords",
    group: "guarded",
    probes: ["row-guard"],
    note:
      "a place with everything but `base_position`. `parseCoords` would read it as 0,0 and " +
      "put a card on a parcel nobody deployed, so the guard drops it in both modes \u{2014} the " +
      "rule schemas/places.ts states for the checking build",
    run: () => {
      const { base_position: _absent, ...noCoords } = placeFull;
      return fetchPlaces(
        {},
        {
          fetchImpl: jsonFetch([
            [/\/api\/places(\?|$)/, { ok: true, data: [noCoords, placeSparse], total: 2 }],
          ]),
        },
      );
    },
  },
  {
    id: "places/fetchPlaces+badEnvelope",
    group: "guarded",
    probes: ["row-guard"],
    note:
      "the service answers 200 with an error body. The envelope schema rejects it in the " +
      "default build; in perf the stub says yes and `data` is simply absent, so the rows are " +
      "read through `field`/`listOf` rather than iterated blind",
    run: () =>
      fetchPlaces(
        {},
        { fetchImpl: jsonFetch([[/\/api\/places(\?|$)/, { error: "upstream unavailable" }]]) },
      ),
  },
  {
    id: "places/fetchCategories+guardedRow",
    group: "guarded",
    probes: ["row-guard"],
    note: "an unnamed category \u{2014} no key, no label, nothing to filter by",
    run: () =>
      fetchCategories({
        fetchImpl: jsonFetch([
          [
            /\/api\/categories(\?|$)/,
            {
              ok: true,
              data: [
                { name: "game", active: true, count: 120, i18n: { en: "Games" } },
                { active: true, count: 7, i18n: { en: "Nameless" } },
              ],
            },
          ],
        ]),
      }),
  },
  {
    id: "events/fetchEvents+guardedRow",
    group: "guarded",
    probes: ["row-guard"],
    note: "an event with no id: nothing to key the card by or address the detail route with",
    run: () => {
      const { id: _absent, ...noId } = eventSparse;
      return fetchEvents(
        {},
        {
          fetchImpl: jsonFetch([[/\/api\/events(\?|$)/, { data: [eventFull, noId], total: 2 }]]),
        },
      );
    },
  },
  {
    id: "events/fetchEventAttendees+guardedRow",
    group: "guarded",
    probes: ["row-guard"],
    note:
      "an attendee row with no user: `isAttending` lowercases `user`, so both modes drop " +
      "the row instead of crashing the attendance read",
    run: () =>
      fetchEventAttendees("ev-1", {
        fetchImpl: jsonFetch([
          [
            /\/api\/events\/[^/]+\/attendees/,
            {
              ok: true,
              data: [
                {
                  event_id: "ev-1",
                  user: "0xabc0000000000000000000000000000000000001",
                  user_name: null,
                  created_at: "2026-08-01T12:00:00Z",
                },
                { event_id: "ev-1", created_at: "2026-08-01T13:00:00Z" },
              ],
            },
          ],
        ]),
      }),
  },
  {
    id: "communities/loadCommunities+guardedRow",
    group: "guarded",
    probes: ["row-guard"],
    note: "a community node with no id \u{2014} un-keyable in the grid, un-fetchable afterwards",
    run: () =>
      loadCommunities(
        {},
        {
          fetchImpl: jsonFetch([
            [
              /\/v1\/communities(\?|$)/,
              { data: { results: [communityFederated, { name: "no id" }], total: 2 } },
            ],
          ]),
        },
      ),
  },
  {
    id: "backpack/parseCatalog+guardedRow",
    group: "guarded",
    probes: ["row-guard"],
    note:
      "a catalog element with no urn: `findWearable` and the owned set both match on it, and " +
      "`byCategory` would file the row under `undefined`",
    run: () => parseCatalog([wearableFull, { name: "orphan" }]),
  },
  {
    id: "backpack/parseLoadout+guardedRow",
    group: "guarded",
    probes: ["row-guard"],
    note:
      "emote-wheel bindings on slot 42 and on the STRING \"2\". `sortLoadout` subtracts slots, " +
      "so either one makes the comparator NaN and puts an emote under a key the user did not " +
      "assign it to",
    run: () =>
      parseLoadout([
        { slot: 1, urn: "urn:decentraland:off-chain:base-emotes:wave", name: "Wave" },
        { slot: 42, urn: "urn:decentraland:off-chain:base-emotes:dab" },
        { slot: "2", urn: "urn:decentraland:off-chain:base-emotes:dance" },
      ]),
  },
  {
    id: "backpack/projectRawEmote+guardedRow",
    group: "guarded",
    probes: ["row-guard"],
    note: "an emote candidate carrying neither urn nor id \u{2014} both modes must answer null",
    run: () => projectRawEmote({ name: "Nameless", emoteDataADR74: { category: "dance" } }),
  },
  {
    id: "notifications/parseNotifications+guardedRow",
    group: "guarded",
    probes: ["row-guard"],
    note:
      "one row with no `type` and one whose `timestamp` is a string. The sort is " +
      "`b.timestamp - a.timestamp`, so keeping the second would reorder the unread list " +
      "around a NaN",
    run: () =>
      parseNotifications({
        notifications: [
          notificationRows[0],
          { ...notificationRows[1], type: undefined },
          { ...notificationRows[1], id: "n-3", timestamp: "2026-07-30T18:10:00Z" },
        ],
      }),
  },

  {
    id: "places/fetchPlaces+uncheckedRow",
    group: "robustness",
    probes: ["bad-row"],
    note:
      "a row the GUARD accepts and the SCHEMA does not \u{2014} `likes` is a string. Perf keeps it " +
      "and renders a card; that difference is exactly what the mode removes. The two real " +
      "parcels have to come back either way",
    run: () =>
      fetchPlaces(
        {},
        {
          fetchImpl: jsonFetch([
            [
              /\/api\/places(\?|$)/,
              {
                ok: true,
                data: [placeFull, { ...placeSparse, id: "loose", likes: "many" }, placeSparse],
                total: 3,
              },
            ],
          ]),
        },
      ),
  },
  {
    id: "places/fetchCategories+badRow",
    group: "robustness",
    probes: ["bad-row"],
    note:
      "a named category row with no i18n object at all: the guard admits it, the schema does " +
      "not, and `normalizePlaceCategory` optional-chains `i18n` so perf renders it unlabelled " +
      "rather than dropping the list",
    run: () =>
      fetchCategories({
        fetchImpl: jsonFetch([
          [
            /\/api\/categories(\?|$)/,
            {
              ok: true,
              data: [
                { name: "game", active: true, count: 120, i18n: { en: "Games" } },
                { name: "broken", active: true, count: 0 },
              ],
            },
          ],
        ]),
      }),
  },
  {
    id: "communities/loadCommunities+badRow",
    group: "robustness",
    probes: ["bad-row"],
    note:
      "a node that is not a community but does carry an id. Nothing maps a community, so perf " +
      "keeping it costs one empty card \u{2014} losing the good ones would not be tolerable",
    run: () =>
      loadCommunities(
        {},
        {
          fetchImpl: jsonFetch([
            [
              /\/v1\/communities(\?|$)/,
              { data: { results: [communityFederated, { id: "not-a-community" }], total: 2 } },
            ],
          ]),
        },
      ),
  },
];

/**
 * Default-mode expectations, checked against literals.
 *
 * Deliberately few: an anchor earns its place only where losing the
 * normalization is a production incident rather than a cosmetic change. Both of
 * these were confirmed blind to the differential -- deleting the rewrite left
 * the harness green at 27/27 while sending thumbnails to the prod CDN.
 *
 * `serviceBase` is called rather than hard-coded so the anchor asserts "the
 * federated host, whatever it is configured to be", not one deployment's URL.
 */
export const ANCHORS: ParityAnchor[] = [
  {
    id: "communities/loadCommunities",
    // loadCommunities resolves Community[], not an envelope.
    select: (out) =>
      ((out as { thumbnailUrl?: string | null }[]) ?? []).map((r) => r.thumbnailUrl ?? null),
    expect: [
      `${serviceBase("communitiesCdn")}/social/communities/${COMMUNITY_ID}/raw-thumbnail.png`,
      null,
      // A host the rewrite must LEAVE ALONE -- it only matches the prod CDN
      // prefix -- so this one is a literal, not a serviceBase() call. Writing it
      // as serviceBase() was wrong and the anchor caught it: that would assert
      // the rewrite fires on a URL it must not touch.
      "https://assets-cdn.decentraland.org/social/communities/3f5a1d90-77b2-4a1c-8a1f-9c2e4b6d8a70/raw-thumbnail.png",
    ],
    why:
      "The prod-CDN row must be rewritten to the federated host, the N/A sentinel must " +
      "become null, and the already-federated row must pass through unchanged. play.catalyst.example.com " +
      "must never reach cdn.decentraland.org; a differential cannot see this because " +
      "deleting the rewrite loses it in BOTH modes.",
  },
];
