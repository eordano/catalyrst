import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { loadCreateScreen, resetCreateScreenCache } from "./create.server";
import { loadCreatorScenes } from "../catalyst/create/index.server";
import { fetchMostActivePlaces } from "../catalyst/places/index";
import { fetchEvents } from "../catalyst/places/events";
import { fetchProfile } from "../catalyst/overlay/profile";

vi.mock("../catalyst/create/index.server", () => ({ loadCreatorScenes: vi.fn() }));
vi.mock("../catalyst/places/index", () => ({ fetchMostActivePlaces: vi.fn() }));
vi.mock("../catalyst/places/events", () => ({ fetchEvents: vi.fn() }));
vi.mock("../catalyst/overlay/profile", () => ({ fetchProfile: vi.fn() }));
vi.mock("@core/lib/content/blog", () => ({ blogPostCards: () => [] }));

const address = "0x1111111111111111111111111111111111111111";
const request = () => new Request("https://sites.test/create", { headers: { cookie: `dcl_wallet=${address}` } });

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  resetCreateScreenCache();
  vi.mocked(loadCreatorScenes).mockResolvedValue([]);
  vi.mocked(fetchMostActivePlaces).mockResolvedValue([]);
  vi.mocked(fetchEvents).mockResolvedValue({ data: [], total: 0 });
  vi.mocked(fetchProfile).mockResolvedValue({ name: "Alice", hasClaimedName: false, description: "" });
});
afterEach(() => { resetCreateScreenCache(); vi.useRealTimers(); });

describe("creator home screen", () => {
  it("returns the viewer's profile in the same response and starts it alongside scenes", async () => {
    let release!: () => void;
    vi.mocked(loadCreatorScenes).mockImplementation(async () => {
      await new Promise<void>((resolve) => { release = resolve; });
      return [];
    });
    const pending = loadCreateScreen(request());
    await vi.advanceTimersByTimeAsync(0);
    expect(fetchProfile).toHaveBeenCalledWith(address, { signal: expect.any(AbortSignal) });
    expect(fetchMostActivePlaces).toHaveBeenCalledTimes(1);
    expect(fetchEvents).toHaveBeenCalledTimes(2);
    release();
    const { data } = await pending;
    expect(data).toMatchObject({ creator: address, profileAddress: address, profileName: "Alice", scenesError: false });
  });

  it("reuses public feeds while rescoping creator and viewer reads", async () => {
    await loadCreateScreen(request());
    await loadCreateScreen(request());
    expect(fetchMostActivePlaces).toHaveBeenCalledTimes(1);
    expect(fetchEvents).toHaveBeenCalledTimes(2);
    expect(loadCreatorScenes).toHaveBeenCalledTimes(2);
    expect(fetchProfile).toHaveBeenCalledTimes(2);
  });

  it("uses active events when trending is slow without a serial fallback request", async () => {
    vi.mocked(fetchEvents).mockImplementation(async ({ list } = {}) => {
      if (list === "trending") return new Promise(() => {});
      return { data: [{ id: "active", name: "Live event", live: true } as never], total: 1 };
    });
    const pending = loadCreateScreen(request());
    await vi.advanceTimersByTimeAsync(750);
    const { data } = await pending;
    expect(data.happenings).toMatchObject([{ id: "active", title: "Live event" }]);
    expect(data.sections.events).toBe("ready");
  });

  it("keeps scenes when the profile fails and exposes that section's failure", async () => {
    vi.mocked(fetchProfile).mockRejectedValue(new Error("offline"));
    const { data } = await loadCreateScreen(request());
    expect(data.scenesError).toBe(false);
    expect(data.profileName).toBe("");
    expect(data.sections.profile).toBe("unavailable");
  });

  it("does not treat the viewed creator as the signed-in viewer", async () => {
    const other = "0x2222222222222222222222222222222222222222";
    const { data } = await loadCreateScreen(new Request(`https://sites.test/create?creator=${other}`, { headers: { cookie: `dcl_wallet=${address}` } }));
    expect(data.creator).toBe(other);
    expect(data.profileAddress).toBe(address);
    expect(loadCreatorScenes).toHaveBeenCalledWith(expect.objectContaining({ creator: other }));
    expect(fetchProfile).toHaveBeenCalledWith(address, expect.anything());
  });
});
