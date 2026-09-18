import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { loadPlayScreen, resetPlayScreenCache } from "@data/lib/screens/play.server";
import { loader } from "./api.screens.v1.play";

const address = "0x1111111111111111111111111111111111111111";
const paths: string[] = [];
function body(path: string) {
  if (path.includes("/profile/")) return { avatars: [{ name: "Alice", avatar: {
    bodyShape: "urn:decentraland:off-chain:base-avatars:BaseMale", wearables: [], emotes: [],
  } }] };
  if (path === "/content/entities/active") return [];
  if (path.includes("/explorer/")) return { elements: [], totalAmount: 0 };
  return { data: [], total: 0 };
}
const request = (headers?: HeadersInit) => new Request(`https://sites.test/api/screens/v1/play?address=${address}`, { headers });

beforeEach(() => {
  vi.useFakeTimers();
  resetPlayScreenCache();
  paths.length = 0;
  vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => {
    const path = new URL(String(input)).pathname;
    paths.push(path);
    return Response.json(body(path));
  }));
});
afterEach(() => { resetPlayScreenCache(); vi.unstubAllGlobals(); vi.unstubAllEnvs(); vi.useRealTimers(); });

describe("play screen", () => {
  it("includes upcoming events only for clients opting in, preserving older streams", async () => {
    const legacy = await loadPlayScreen(request());
    expect(legacy.data.sections).not.toHaveProperty("upcoming");
    const url = new URL(request().url);
    url.searchParams.set("include", "upcoming");
    const response = await loader({ request: new Request(url, { headers: { accept: "application/x-ndjson" } }) });
    const frames = (await response.text()).trim().split("\n").map((line) => JSON.parse(line));
    expect(frames.filter((frame) => frame.section === "upcoming")).toMatchObject([{ result: { status: "ready" } }]);
    expect(paths.filter((path) => path.startsWith("/events/"))).toHaveLength(2);
  });

  it("shares public feeds across accounts while loading each account's inventory separately", async () => {
    await loadPlayScreen(request());
    await loadPlayScreen(new Request("https://sites.test/api/screens/v1/play?address=0x2222222222222222222222222222222222222222"));
    expect(paths.filter((path) => path.startsWith("/places/") || path.startsWith("/events/"))).toHaveLength(3);
    expect(paths.filter((path) => path.includes("/profile/"))).toHaveLength(2);
    expect(paths.filter((path) => path.endsWith("/wearables"))).toHaveLength(2);
  });

  it("preserves the source timestamp on cache hits and refreshes after 30 seconds", async () => {
    const first = await loadPlayScreen(request());
    await vi.advanceTimersByTimeAsync(20_000);
    const warm = await loadPlayScreen(request());
    expect(warm.data.sections.places.updatedAt).toBe(first.data.sections.places.updatedAt);
    expect(paths.filter((path) => path.startsWith("/places/"))).toHaveLength(2);
    await vi.advanceTimersByTimeAsync(10_000);
    const stale = await loadPlayScreen(request());
    expect(stale.data.sections.places.updatedAt).toBe(first.data.sections.places.updatedAt);
    expect(stale.data.sections.places).toMatchObject({ refreshing: true });
    await vi.advanceTimersByTimeAsync(0);
    const refreshed = await loadPlayScreen(request());
    expect(refreshed.data.sections.places.updatedAt).toBeGreaterThan(first.data.sections.places.updatedAt!);
    expect(paths.filter((path) => path.startsWith("/places/"))).toHaveLength(4);
  });

  it("does not retain failed public feeds or share them between backends", async () => {
    let failed = false;
    vi.mocked(fetch).mockImplementation(async (input) => {
      const url = new URL(String(input));
      paths.push(url.pathname);
      if (url.searchParams.get("only_highlighted") === "true" && !failed) {
        failed = true;
        throw new Error("offline");
      }
      return Response.json(body(url.pathname));
    });
    const first = await loadPlayScreen(new Request("https://sites.test/api/screens/v1/play"));
    expect(first.data.sections.featured.status).toBe("unavailable");
    expect((await loadPlayScreen(request())).data.sections.featured.status).toBe("ready");
    const previous = paths.length;
    vi.stubEnv("CATALYST_URL", "https://another.test");
    await loadPlayScreen(request());
    expect(paths.slice(previous).some((path) => path.startsWith("/places/") || path.startsWith("/events/"))).toBe(true);
  });

  it("coalesces concurrent public reads and survives one visitor cancelling", async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    vi.mocked(fetch).mockImplementation(async (input) => {
      const path = new URL(String(input)).pathname;
      paths.push(path);
      await gate;
      return Response.json(body(path));
    });
    const cancel = new AbortController();
    const first = loadPlayScreen(new Request(request(), { signal: cancel.signal }));
    const rejected = expect(first).rejects.toMatchObject({ name: "AbortError" });
    const second = loadPlayScreen(request());
    await vi.advanceTimersByTimeAsync(0);
    expect(paths.filter((path) => path.startsWith("/places/") || path.startsWith("/events/"))).toHaveLength(3);
    cancel.abort();
    await rejected;
    release();
    expect((await second).data.sections.places.status).toBe("ready");
  });

  it("starts independent reads together and shares one equipped profile across both inventories", async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    vi.mocked(fetch).mockImplementation(async (input) => {
      const path = new URL(String(input)).pathname;
      paths.push(path);
      await gate;
      return Response.json(body(path));
    });
    const pending = loadPlayScreen(request());
    await vi.advanceTimersByTimeAsync(0);
    expect(paths.some((path) => path.startsWith("/places/"))).toBe(true);
    expect(paths.some((path) => path.startsWith("/events/"))).toBe(true);
    expect(paths.filter((path) => path.includes("/profile/"))).toHaveLength(1);
    expect(paths).toContain(`/lambdas/explorer/${address}/emotes`);
    expect(paths).toContain(`/lambdas/explorer/${address}/wearables`);
    release();
    const { data } = await pending;
    expect(data.sections.wearables.data?.equipped.name).toBe("Alice");
    expect(Object.values(data.sections).every((section) => section.status === "ready")).toBe(true);
  });

  it("streams completed sections before an unrelated request resolves", async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    vi.mocked(fetch).mockImplementation(async (input) => {
      const path = new URL(String(input)).pathname;
      if (path.includes("/wearables")) await gate;
      return Response.json(body(path));
    });
    const response = await loader({ request: request({ accept: "application/x-ndjson" }) });
    expect(response.headers.get("X-Accel-Buffering")).toBe("no");
    const reader = response.body!.getReader();
    const first = await reader.read();
    const message = JSON.parse(new TextDecoder().decode(first.value));
    expect(message.version).toBe(1);
    expect(message.section).not.toBe("wearables");
    release();
    let remaining = "";
    while (true) {
      const next = await reader.read();
      if (next.done) break;
      remaining += new TextDecoder().decode(next.value);
    }
    expect(remaining).toContain('"section":"wearables"');
    expect(remaining).toContain('"done":true');
  });

  it("bounds a stalled section while keeping usable sections", async () => {
    vi.mocked(fetch).mockImplementation(async (input) => {
      const path = new URL(String(input)).pathname;
      if (path.startsWith("/events/")) return new Promise(() => {});
      return Response.json(body(path));
    });
    const pending = loadPlayScreen(request());
    await vi.advanceTimersByTimeAsync(3_000);
    const { data } = await pending;
    expect(data.sections.events.status).toBe("unavailable");
    expect(data.sections.wearables.status).toBe("ready");
  });

  it("aborts account work on disconnect while allowing bounded public refreshes to finish", async () => {
    const signals: AbortSignal[] = [];
    vi.mocked(fetch).mockImplementation(async (_input, init) => {
      if (init?.signal) signals.push(init.signal);
      return new Promise(() => {});
    });
    const response = await loader({ request: request({ accept: "application/x-ndjson" }) });
    await vi.advanceTimersByTimeAsync(0);
    await response.body!.cancel();
    await vi.advanceTimersByTimeAsync(0);
    const aborted = signals.filter((signal) => signal.aborted).length;
    expect(aborted).toBeGreaterThan(0);
    expect(aborted).toBeLessThan(signals.length);
    await vi.advanceTimersByTimeAsync(3_000);
    expect(signals.every((signal) => signal.aborted)).toBe(true);
  });

  it("rejects invalid wallet inputs before starting any work", async () => {
    await expect(loadPlayScreen(new Request("https://sites.test/api/screens/v1/play?address=invalid")))
      .rejects.toMatchObject({ status: 400 });
    expect(fetch).not.toHaveBeenCalled();
  });
});
