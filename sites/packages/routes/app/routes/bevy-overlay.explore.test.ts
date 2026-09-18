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
import { resetCatalogRailCache } from "@data/lib/catalyst/marketplace/catalog-rails.server";
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
  resetCatalogRailCache();
  vi.spyOn(track, "trackExposure").mockImplementation(() => {});
  placesMock.mockResolvedValue({ data: [], total: 0 } as never);
  catalogMock.mockResolvedValue({ data: [] } as never);
  seasonsMock.mockResolvedValue(null);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("GET /bevy-overlay/explore", () => {
  it("resolves the tab and loads only what that tab shows", async () => {
    expect((await dataFrom("?tab=bogus")).tab).toBe("places");
    expect((await dataFrom()).tab).toBe("places");
    expect(placesMock).toHaveBeenCalledTimes(2);
    expect(catalogMock).not.toHaveBeenCalled();

    expect((await dataFrom("?tab=marketplace")).tab).toBe("marketplace");
    expect(catalogMock).toHaveBeenCalledTimes(1);
    expect(placesMock).toHaveBeenCalledTimes(2);

    expect((await dataFrom("?tab=reel")).tab).toBe("reel");
    expect(placesMock).toHaveBeenCalledTimes(2);
    expect(catalogMock).toHaveBeenCalledTimes(1);
  });

  it("marks a dead seasons read failed, not an empty season, and builds the hub from a live one", async () => {
    const dead = await dataFrom("?tab=credits");
    expect(dead.tab).toBe("credits");
    expect(dead.credits).toEqual({ hub: null, failed: true });

    seasonsMock.mockResolvedValue({
      currentSeason: { season: { name: "Season One" }, week: { weekNumber: 2, secondsRemaining: 3600 } },
    } as never);
    const live = await dataFrom("?tab=credits");
    expect(live.credits.failed).toBe(false);
    expect(live.credits.hub).toMatchObject({ seasonName: "Season One", weekNumber: 2 });
  });

  it("tells a failed places read from a genuinely empty one", async () => {
    expect((await dataFrom("?tab=places")).places).toEqual({ items: [], failed: false });
    placesMock.mockRejectedValue(new Error("down"));
    expect((await dataFrom("?tab=places")).places).toEqual({ items: [], failed: true });
  });

  it("tells a failed catalog read from a genuinely empty shop", async () => {
    expect((await dataFrom("?tab=marketplace")).collectibles).toEqual({ items: [], failed: false });
    resetCatalogRailCache();
    catalogMock.mockRejectedValue(new Error("down"));
    expect((await dataFrom("?tab=marketplace")).collectibles).toEqual({ items: [], failed: true });
  });
});
