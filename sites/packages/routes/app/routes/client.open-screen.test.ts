import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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
import { resetRuntimeFlagCache } from "@core/lib/experiments/flags";
import * as track from "@core/lib/telemetry/track";
import { loader } from "./client.open-screen";

const activeMock = vi.mocked(fetchMostActivePlaces);
const loadMock = vi.mocked(loadPlaces);

const TELEMETRY = "https://telemetry.example.com";
const fetchMock = vi.fn();

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

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

function overrideRow(row: unknown) {
  process.env.TELEMETRY_URL = TELEMETRY;
  vi.stubGlobal("fetch", fetchMock);
  fetchMock.mockImplementation((url: string) =>
    Promise.resolve(jsonResponse(url.includes("/dash/flags") ? { flags: {} } : row)),
  );
}

let exposure: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  vi.clearAllMocks();
  resetRuntimeFlagCache();
  delete process.env.TELEMETRY_URL;
  delete process.env.OPEN_SCREEN_EXPERIMENT;
  exposure = vi.spyOn(track, "trackExposure").mockImplementation(() => {});
  activeMock.mockResolvedValue([]);
  loadMock.mockResolvedValue({ data: [] } as never);
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  delete process.env.TELEMETRY_URL;
  delete process.env.OPEN_SCREEN_EXPERIMENT;
});

describe("GET /client/open-screen", () => {
  it("genesis arm redirects to /places when no scene is live, and stays put when one is, counting nothing", async () => {
    const thrown = await caught(loader(args("?arm=genesis")));
    expect(thrown).toBeInstanceOf(Response);
    const res = thrown as Response;
    expect(res.status).toBe(302);
    expect(res.headers.get("Location")).toBe("/places");
    expect(exposure).not.toHaveBeenCalled();

    activeMock.mockResolvedValue([live("a", 5), live("b", 99)]);
    expect(await caught(loader(args("?arm=genesis")))).toBeUndefined();
    expect(exposure).not.toHaveBeenCalled();
  });

  it("serves base with no exposure while the draft is off, even after a no-op override row", async () => {
    expect(await caught(loader(args()))).toBeUndefined();
    expect(loadMock).toHaveBeenCalledTimes(1);
    expect(activeMock).not.toHaveBeenCalled();
    expect(exposure).not.toHaveBeenCalled();

    overrideRow({ killed: false, variant: null, flags: {} });
    expect(await caught(loader(args()))).toBeUndefined();
    expect(loadMock).toHaveBeenCalledTimes(2);
    expect(exposure).not.toHaveBeenCalled();
  });

  it("forced previews never count an exposure, active experiment or not", async () => {
    for (const search of ["?arm=base", "?arm=three-cards", "?variant=client_open_screen:three-cards"]) {
      expect(await caught(loader(args(search))), search).toBeUndefined();
      expect(exposure, search).not.toHaveBeenCalled();
    }

    overrideRow({ killed: false, variant: "three-cards", flags: {} });
    expect(await caught(loader(args("?arm=base")))).toBeUndefined();
    expect(exposure).not.toHaveBeenCalled();

    process.env.OPEN_SCREEN_EXPERIMENT = "client_open_screen";
    activeMock.mockResolvedValue([live("a", 5)]);
    expect(await caught(loader(args("?arm=three-cards")))).toBeUndefined();
    expect(exposure).not.toHaveBeenCalled();
  });

  it("an active flag buckets the session and a variant pin serves that arm, each counting one exposure", async () => {
    overrideRow({ killed: false, variant: null, flags: { active: true } });
    activeMock.mockResolvedValue([live("a", 5)]);
    expect(await caught(loader(args()))).toBeUndefined();
    expect(exposure).toHaveBeenCalledTimes(1);
    expect(exposure).toHaveBeenCalledWith(expect.objectContaining({ experimentKey: "client_open_screen" }));

    resetRuntimeFlagCache();
    overrideRow({ killed: false, variant: "three-cards", flags: {} });
    expect(await caught(loader(args()))).toBeUndefined();
    expect(exposure).toHaveBeenCalledTimes(2);
    expect(exposure).toHaveBeenLastCalledWith(expect.objectContaining({ variant: "three-cards" }));
  });

  it("a kill row serves base and stops exposures", async () => {
    overrideRow({ killed: true, variant: null, flags: {} });
    expect(await caught(loader(args()))).toBeUndefined();
    expect(loadMock).toHaveBeenCalledTimes(1);
    expect(exposure).not.toHaveBeenCalled();
  });

  it("the OPEN_SCREEN_EXPERIMENT env var still activates the experiment and counts the exposure", async () => {
    process.env.OPEN_SCREEN_EXPERIMENT = "client_open_screen";
    activeMock.mockResolvedValue([live("a", 5)]);
    expect(await caught(loader(args()))).toBeUndefined();
    expect(exposure).toHaveBeenCalledTimes(1);
  });
});
