/**
 * Story/test fixtures for the provenance vocabulary.
 *
 * These exist so Storybook can render every state deterministically. They are
 * **story data, not page data**: no component in this library may import this
 * module, and no default prop may reference it. A page with no data renders an
 * unavailable state, never a fixture.
 */

import {
  DEFAULT_CADENCE_SECONDS,
  live,
  noSample,
  sampled,
  snapshot,
  unavailable,
  unbuilt,
  type Datum,
} from "./datum";

/** Pinned clock so `2m ago` is `2m ago` in every screenshot. */
export const FIXTURE_NOW = Date.parse("2026-07-31T14:03:22Z");

export const at = (msAgo: number): string =>
  new Date(FIXTURE_NOW - msAgo).toISOString();

const READ_AT = at(0);

export const PRESENCE_ENDPOINT =
  "GET catalyst.example.com/presence/current/worlds";
export const LIVE_DATA_ENDPOINT =
  "GET worlds-content-server.decentraland.org/live-data";

export const datumFixtures = {
  live: live(5, LIVE_DATA_ENDPOINT, READ_AT),
  sampled: sampled(
    2,
    PRESENCE_ENDPOINT,
    at(2 * 60_000),
    DEFAULT_CADENCE_SECONDS,
    READ_AT,
  ),
  stale: sampled(
    2,
    PRESENCE_ENDPOINT,
    at(22 * 60_000),
    DEFAULT_CADENCE_SECONDS,
    READ_AT,
  ),
  snapshot: snapshot(
    1_214,
    "GET creators-data.example.com/api/worlds/petbarn.dcl.eth/metrics",
    at(6 * 86_400_000),
    "metabase",
    READ_AT,
  ),
  realZero: sampled(
    0,
    PRESENCE_ENDPOINT,
    at(2 * 60_000),
    DEFAULT_CADENCE_SECONDS,
    READ_AT,
  ),
  noSample: noSample(
    PRESENCE_ENDPOINT,
    at(2 * 60_000),
    "No sample: this world was not live at the last snapshot (14:01:22 UTC). Not the same as zero.",
  ),
  unavailable: unavailable(
    LIVE_DATA_ENDPOINT,
    503,
    "GET worlds-content-server.decentraland.org/live-data returned 503.",
  ),
  unbuilt: unbuilt(
    "Sessions & retention",
    "Presence persists addresses (scene_occupancy.addresses) but its HTTP API returns counts only. The client half of session/retention analytics exists in this repo with a generated zod model and a drift gate; the server route /creators/me/scenes/stats 404s.",
    "the headcounts above are the whole picture.",
  ),
} satisfies Record<string, Datum<number | string>>;

export type DatumFixtureKey = keyof typeof datumFixtures;

export const DATUM_FIXTURE_KEYS = Object.keys(
  datumFixtures,
) as DatumFixtureKey[];
