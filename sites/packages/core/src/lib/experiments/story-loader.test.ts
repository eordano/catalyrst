import { afterEach, describe, expect, it, vi } from "vitest";

import { resetRuntimeFlagCache } from "./flags";
import { sidLoader, storyLoaderWith } from "./story-loader";

function req(url: string, cookie?: string): Request {
  return new Request(url, { headers: cookie ? { cookie } : {} });
}

function cookies(base: ReturnType<typeof sidLoader>): string[] {
  const result = base.wrap({}) as { init?: { headers?: HeadersInit } | null };
  const headers = result.init?.headers;
  if (!headers) return [];
  if (headers instanceof Headers) return headers.getSetCookie();
  const raw = (headers as Record<string, string>)["Set-Cookie"];
  return raw ? [raw] : [];
}

describe("sidLoader cookie scope", () => {
  it("first mint under *.catalyst.example.com is wide-scope from the start; elsewhere it stays host-scope", () => {
    const wide = sidLoader(req("https://studio.catalyst.example.com/studio"));
    expect(wide.created).toBe(true);
    const wideSet = cookies(wide);
    expect(wideSet).toHaveLength(1);
    expect(wideSet[0]).toContain(`sid=${wide.sid}`);
    expect(wideSet[0]).toContain("Domain=catalyst.example.com");

    const host = sidLoader(req("https://sites.example.com/"));
    expect(host.created).toBe(true);
    const hostSet = cookies(host);
    expect(hostSet).toHaveLength(1);
    expect(hostSet[0]).not.toContain("Domain=");
  });

  it("a split jar under the shared parent converges: wide winner plus host-scope expiry", () => {
    const base = sidLoader(req("https://studio.catalyst.example.com/studio", "sid=stale; sid=winner"));
    expect(base.created).toBe(false);
    expect(base.sid).toBe("winner");
    const set = cookies(base);
    expect(set).toHaveLength(2);
    expect(set[0]).toContain("sid=winner");
    expect(set[0]).toContain("Domain=catalyst.example.com");
    expect(set[1].startsWith("sid=;")).toBe(true);
    expect(set[1]).toContain("Max-Age=0");
    expect(set[1]).not.toContain("Domain=");
  });

  it("a single existing sid, or a split jar off the shared parent, sets nothing", () => {
    const single = sidLoader(req("https://studio.catalyst.example.com/studio", "sid=only"));
    expect(single.created).toBe(false);
    expect(cookies(single)).toHaveLength(0);
    expect(cookies(sidLoader(req("https://sites.example.com/", "sid=a; sid=b")))).toHaveLength(0);
  });
});

describe("storyLoaderWith", () => {
  const FALLBACK = { variant: "index_grid", flags: {}, experimentKey: "lp_blog_index" };
  const telemetryUrl = process.env.TELEMETRY_URL;

  afterEach(() => {
    vi.restoreAllMocks();
    resetRuntimeFlagCache();
    if (telemetryUrl === undefined) delete process.env.TELEMETRY_URL;
    else process.env.TELEMETRY_URL = telemetryUrl;
  });

  it("starts the data load before the flag reads answer", async () => {
    process.env.TELEMETRY_URL = "https://tel.test";
    resetRuntimeFlagCache();
    let release!: () => void;
    const loadStarted = new Promise<void>((r) => (release = r));
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(async () => {
      await loadStarted;
      return new Response("null", { status: 200 });
    });
    const out = await storyLoaderWith(
      req("https://sites.example.com/blog", "sid=only"),
      "misc/blog",
      FALLBACK,
      async (base) => {
        release();
        return { sid: base.sid };
      },
      { skipExposure: true },
    );
    expect(fetchSpy).toHaveBeenCalled();
    expect(out.data).toEqual({ sid: "only" });
    expect(out.sid).toBe("only");
    expect(out.assignment.experimentKey).toBe("lp_blog_index");
  });

  it("propagates a failed data load", async () => {
    delete process.env.TELEMETRY_URL;
    await expect(
      storyLoaderWith(req("https://sites.example.com/blog"), "misc/blog", FALLBACK, () =>
        Promise.reject(new Error("upstream down")),
      ),
    ).rejects.toThrow("upstream down");
  });
});
