import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The route pulls in the OpenScreen component (which imports @ui + CSS) only as
// the default export the Component renders -- the loader never calls it. Stub it
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

// Point the loader at a stubbed flags service whose /dash/experiments row for
// client_open_screen is `row` (and whose /dash/flags map is empty).
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

describe("GET /client/open-screen -- genesis arm", () => {
  it("no live scene -> redirects to /places BEFORE counting an exposure", async () => {
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

  it("live scene present -> no redirect (forced preview still counts nothing)", async () => {
    activeMock.mockResolvedValue([live("a", 5), live("b", 99)]);
    const thrown = await caught(loader(args("?arm=genesis")));
    expect(thrown).toBeUndefined();
    expect(exposure).not.toHaveBeenCalled();
  });
});

describe("GET /client/open-screen -- draft (no activation)", () => {
  it("serves base with no exposure", async () => {
    const thrown = await caught(loader(args()));
    expect(thrown).toBeUndefined();
    expect(loadMock).toHaveBeenCalledTimes(1);
    expect(activeMock).not.toHaveBeenCalled();
    expect(exposure).not.toHaveBeenCalled();
  });

  it("a no-op override row (UI save with nothing set) keeps the draft off", async () => {
    overrideRow({ killed: false, variant: null, flags: {} });
    const thrown = await caught(loader(args()));
    expect(thrown).toBeUndefined();
    expect(loadMock).toHaveBeenCalledTimes(1);
    expect(exposure).not.toHaveBeenCalled();
  });
});

describe("GET /client/open-screen -- forced previews never count exposure", () => {
  it("?arm=base loads the browse grid without an exposure", async () => {
    const thrown = await caught(loader(args("?arm=base")));
    expect(thrown).toBeUndefined();
    expect(loadMock).toHaveBeenCalledTimes(1);
    expect(activeMock).not.toHaveBeenCalled();
    expect(exposure).not.toHaveBeenCalled();
  });

  it("?arm=three-cards stays (no redirect) without an exposure", async () => {
    activeMock.mockResolvedValue([]);
    const thrown = await caught(loader(args("?arm=three-cards")));
    expect(thrown).toBeUndefined();
    expect(exposure).not.toHaveBeenCalled();
  });

  it("?variant=client_open_screen:three-cards is a preview too", async () => {
    const thrown = await caught(
      loader(args("?variant=client_open_screen:three-cards")),
    );
    expect(thrown).toBeUndefined();
    expect(exposure).not.toHaveBeenCalled();
  });

  it("a forced arm during an ACTIVE experiment still counts nothing", async () => {
    overrideRow({ killed: false, variant: "three-cards", flags: {} });
    const thrown = await caught(loader(args("?arm=base")));
    expect(thrown).toBeUndefined();
    expect(exposure).not.toHaveBeenCalled();
  });
});

describe("GET /client/open-screen -- runtime-flag activation", () => {
  it("{flags:{active:true}} buckets the session and counts the exposure", async () => {
    overrideRow({ killed: false, variant: null, flags: { active: true } });
    activeMock.mockResolvedValue([live("a", 5)]);
    const thrown = await caught(loader(args()));
    expect(thrown).toBeUndefined();
    expect(exposure).toHaveBeenCalledTimes(1);
    expect(exposure).toHaveBeenCalledWith(
      expect.objectContaining({ experimentKey: "client_open_screen" }),
    );
  });

  it("a variant pin serves that arm to everyone and counts the exposure", async () => {
    overrideRow({ killed: false, variant: "three-cards", flags: {} });
    const thrown = await caught(loader(args()));
    expect(thrown).toBeUndefined();
    expect(exposure).toHaveBeenCalledTimes(1);
    expect(exposure).toHaveBeenCalledWith(
      expect.objectContaining({ variant: "three-cards" }),
    );
  });

  it("a kill row serves base and stops exposures", async () => {
    overrideRow({ killed: true, variant: null, flags: {} });
    const thrown = await caught(loader(args()));
    expect(thrown).toBeUndefined();
    expect(loadMock).toHaveBeenCalledTimes(1);
    expect(exposure).not.toHaveBeenCalled();
  });
});

describe("GET /client/open-screen -- env-var activation (back-compat)", () => {
  it("OPEN_SCREEN_EXPERIMENT set counts an exposure for the resolved arm", async () => {
    process.env.OPEN_SCREEN_EXPERIMENT = "client_open_screen";
    activeMock.mockResolvedValue([live("a", 5)]);
    const thrown = await caught(loader(args()));
    expect(thrown).toBeUndefined();
    expect(exposure).toHaveBeenCalledTimes(1);
  });

  it("env-var active plus a forced ?arm= still counts nothing", async () => {
    process.env.OPEN_SCREEN_EXPERIMENT = "client_open_screen";
    activeMock.mockResolvedValue([live("a", 5)]);
    const thrown = await caught(loader(args("?arm=three-cards")));
    expect(thrown).toBeUndefined();
    expect(exposure).not.toHaveBeenCalled();
  });
});
