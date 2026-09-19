import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { loadPlayScreen } from "@data/lib/screens/play.server";
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
  paths.length = 0;
  vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => {
    const path = new URL(String(input)).pathname;
    paths.push(path);
    return Response.json(body(path));
  }));
});
afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });

describe("play screen", () => {
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
    expect(paths).toHaveLength(8);
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

  it("aborts upstream work when the stream consumer disconnects", async () => {
    const signals: AbortSignal[] = [];
    vi.mocked(fetch).mockImplementation(async (_input, init) => {
      if (init?.signal) signals.push(init.signal);
      return new Promise(() => {});
    });
    const response = await loader({ request: request({ accept: "application/x-ndjson" }) });
    await vi.advanceTimersByTimeAsync(0);
    expect(signals).toHaveLength(8);
    await response.body!.cancel();
    await vi.advanceTimersByTimeAsync(0);
    expect(signals.every((signal) => signal.aborted)).toBe(true);
  });

  it("rejects invalid wallet inputs before starting any work", async () => {
    await expect(loadPlayScreen(new Request("https://sites.test/api/screens/v1/play?address=invalid")))
      .rejects.toMatchObject({ status: 400 });
    expect(fetch).not.toHaveBeenCalled();
  });
});
