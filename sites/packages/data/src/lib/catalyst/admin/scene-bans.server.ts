
import fs from "node:fs";
import path from "node:path";

import { controlStatus } from "./control-availability";
import type { Unavailable } from "./availability";
import { PlaceRefSchema, type PlaceRef } from "./scene-bans";

const FIXTURE = path.join(
  process.cwd(),
  "packages",
  "data",
  "src",
  "fixtures",
  "operator-scene-bans.json",
);

type FixtureShape = {
  default_owner?: string;
  places?: unknown[];
  bans?: Record<string, unknown>;
};

function readFixture(): FixtureShape | null {
  try {
    return JSON.parse(fs.readFileSync(FIXTURE, "utf8")) as FixtureShape;
  } catch {
    return null;
  }
}

export type OperatorPlacesFixture = {
  places: PlaceRef[];
  owner: string;
  synthetic: true;
  unreadable: boolean;
};

export function loadOperatorPlaces(): OperatorPlacesFixture {
  const fx = readFixture();
  const places: PlaceRef[] = [];
  for (const row of fx?.places ?? []) {
    const parsed = PlaceRefSchema.safeParse(row);
    if (parsed.success) places.push(parsed.data);
  }
  return {
    places,
    owner: fx?.default_owner ?? "",
    synthetic: true,
    unreadable: fx === null,
  };
}

export function loadSceneBansPage(_placeId: string): Unavailable {
  return controlStatus("sceneBans.list") as Unavailable;
}

export function banInScene(): Unavailable {
  return controlStatus("sceneBans.ban") as Unavailable;
}

export function unbanInScene(): Unavailable {
  return controlStatus("sceneBans.unban") as Unavailable;
}
