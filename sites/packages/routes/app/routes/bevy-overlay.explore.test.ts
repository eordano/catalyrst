import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@data/lib/catalyst/places/index.server", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@data/lib/catalyst/places/index.server")>()),
  loadPlaces: vi.fn(),
}));
vi.mock("@data/lib/catalyst/marketplace/index", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@data/lib/catalyst/marketplace/index")>()),
  fetchCatalog: vi.fn(),
}));
vi.mock("@data/lib/catalyst/marketplace/credits.server", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@data/lib/catalyst/marketplace/credits.server")>()),
  loadSeasons: vi.fn(),
}));

import { loadPlaces } from "@data/lib/catalyst/places/index.server";
import { fetchCatalog } from "@data/lib/catalyst/marketplace/index";
import { loadSeasons } from "@data/lib/catalyst/marketplace/credits.server";
import * as track from "@core/lib/telemetry/track";
import { loader } from "./bevy-overlay.explore";

const placesMock = vi.mocked(loadPlaces);
const catalogMock = vi.mocked(fetchCatalog);
const seasonsMock = vi.mocked(loadSeasons);

function get(search = "") {
  return {
    request: new Request(`https://sites.test/bevy-overlay/explore${search}`),
    params: {},
    context: {} as never,
  };
}

async function dataFrom(search = "") {
  const res = await loader(get(search) as never);
  return res.data as {
    tab: string;
    places: { items: unknown[]; failed: boolean };
    collectibles: { items: unknown[]; failed: boolean };
    credits: { hub: unknown; failed: boolean };
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.spyOn(track, "trackExposure").mockImplementation(() => {});
  placesMock.mockResolvedValue({ data: [], total: 0 } as never);
  catalogMock.mockResolvedValue({ data: [] } as never);
  seasonsMock.mockResolvedValue(null);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("GET /bevy-overlay/explore", () => {
  it("collapses an unknown tab to places", async () => {
    const d = await dataFrom("?tab=bogus");
    expect(d.tab).toBe("places");
    expect(placesMock).toHaveBeenCalledTimes(1);
    expect(catalogMock).not.toHaveBeenCalled();
  });

  it("defaults a missing tab to places", async () => {
    const d = await dataFrom();
    expect(d.tab).toBe("places");
    expect(placesMock).toHaveBeenCalledTimes(1);
  });

  it("keeps the marketplace tab and loads its catalog", async () => {
    const d = await dataFrom("?tab=marketplace");
    expect(d.tab).toBe("marketplace");
    expect(catalogMock).toHaveBeenCalledTimes(1);
    expect(placesMock).not.toHaveBeenCalled();
  });

  it("keeps the reel tab without loading places or catalog", async () => {
    const d = await dataFrom("?tab=reel");
    expect(d.tab).toBe("reel");
    expect(placesMock).not.toHaveBeenCalled();
    expect(catalogMock).not.toHaveBeenCalled();
  });

  it("marks a dead seasons read failed, not an empty season", async () => {
    const d = await dataFrom("?tab=credits");
    expect(d.tab).toBe("credits");
    expect(d.credits).toEqual({ hub: null, failed: true });
  });

  it("builds the hub from a live seasons read", async () => {
    seasonsMock.mockResolvedValue({
      currentSeason: {
        season: { name: "Season One" },
        week: { weekNumber: 2, secondsRemaining: 3600 },
      },
    } as never);
    const d = await dataFrom("?tab=credits");
    expect(d.credits.failed).toBe(false);
    expect(d.credits.hub).toMatchObject({ seasonName: "Season One", weekNumber: 2 });
  });

  it("marks a failed places read failed, not empty", async () => {
    placesMock.mockRejectedValue(new Error("down"));
    const d = await dataFrom("?tab=places");
    expect(d.places).toEqual({ items: [], failed: true });
  });

  it("keeps a genuinely empty places read unfailed", async () => {
    const d = await dataFrom("?tab=places");
    expect(d.places).toEqual({ items: [], failed: false });
  });

  it("marks a failed catalog read failed, not an empty shop", async () => {
    catalogMock.mockRejectedValue(new Error("down"));
    const d = await dataFrom("?tab=marketplace");
    expect(d.collectibles).toEqual({ items: [], failed: true });
  });

  it("keeps a genuinely empty catalog unfailed", async () => {
    const d = await dataFrom("?tab=marketplace");
    expect(d.collectibles).toEqual({ items: [], failed: false });
  });
});
