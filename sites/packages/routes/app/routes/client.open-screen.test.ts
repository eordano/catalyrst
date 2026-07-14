import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The route pulls in the OpenScreen component (which imports @ui + CSS) only as
// the default export the Component renders — the loader never calls it. Stub it
// so this loader test doesn't drag the whole presentation/@ui graph in.
vi.mock("@features/stories/client/open-screen/OpenScreen", () => ({
  default: () => null,
}));
vi.mock("@data/lib/catalyst/places/index", () => ({
  fetchMostActivePlaces: vi.fn(),
}));
vi.mock("@data/lib/catalyst/places/index.server", () => ({
  loadPlaces: vi.fn(),
}));

import { fetchMostActivePlaces, type Place } from "@data/lib/catalyst/places/index";
import { loadPlaces } from "@data/lib/catalyst/places/index.server";
import * as track from "@core/lib/telemetry/track";
import { loader } from "./client.open-screen";

const activeMock = vi.mocked(fetchMostActivePlaces);
const loadMock = vi.mocked(loadPlaces);

function args(search = ""): Parameters<typeof loader>[0] {
  return {
    request: new Request(`https://sites.test/client/open-screen${search}`),
    params: {},
    context: {} as never,
  } as unknown as Parameters<typeof loader>[0];
}

function live(id: string, user_count: number): Place {
  return { id, title: id, base_position: "0,0", user_count } as unknown as Place;
}

async function caught(promise: Promise<unknown>): Promise<unknown> {
  try {
    await promise;
    return undefined;
  } catch (e) {
    return e;
  }
}

let exposure: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  vi.clearAllMocks();
  exposure = vi.spyOn(track, "trackExposure").mockImplementation(() => {});
  activeMock.mockResolvedValue([]);
  loadMock.mockResolvedValue({ data: [] } as never);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("GET /client/open-screen — genesis arm", () => {
  it("no live scene → redirects to /places BEFORE counting an exposure", async () => {
    activeMock.mockResolvedValue([]);
    const thrown = await caught(loader(args("?arm=genesis")));
    expect(thrown).toBeInstanceOf(Response);
    const res = thrown as Response;
    expect(res.status).toBe(302);
    expect(res.headers.get("Location")).toBe("/places");
    // The reorder guarantee: a session that can never see the genesis screen is
    // not diluted into the arm's exposure denominator.
    expect(exposure).not.toHaveBeenCalled();
  });

  it("live scene present → no redirect, and the genesis exposure IS counted", async () => {
    activeMock.mockResolvedValue([live("a", 5), live("b", 99)]);
    const thrown = await caught(loader(args("?arm=genesis")));
    expect(thrown).toBeUndefined();
    expect(exposure).toHaveBeenCalledTimes(1);
    expect(exposure).toHaveBeenCalledWith(expect.objectContaining({ variant: "genesis" }));
  });
});

describe("GET /client/open-screen — other arms never redirect", () => {
  it("base → loads the browse grid and counts a base exposure", async () => {
    const thrown = await caught(loader(args("?arm=base")));
    expect(thrown).toBeUndefined();
    expect(loadMock).toHaveBeenCalledTimes(1);
    expect(activeMock).not.toHaveBeenCalled();
    expect(exposure).toHaveBeenCalledWith(expect.objectContaining({ variant: "base" }));
  });

  it("three-cards with no live scene stays (no redirect) and counts its exposure", async () => {
    activeMock.mockResolvedValue([]);
    const thrown = await caught(loader(args("?arm=three-cards")));
    expect(thrown).toBeUndefined();
    expect(exposure).toHaveBeenCalledWith(
      expect.objectContaining({ variant: "three-cards" }),
    );
  });
});
