import { describe, expect, it } from "vitest";

import { sidLoader } from "./story-loader";

function req(url: string, cookie?: string): Request {
  return new Request(url, { headers: cookie ? { cookie } : {} });
}

/** The Set-Cookie values a sidLoader's wrap() attached, in emission order. */
function cookies(base: ReturnType<typeof sidLoader>): string[] {
  const result = base.wrap({}) as { init?: { headers?: HeadersInit } | null };
  const headers = result.init?.headers;
  if (!headers) return [];
  if (headers instanceof Headers) return headers.getSetCookie();
  const raw = (headers as Record<string, string>)["Set-Cookie"];
  return raw ? [raw] : [];
}

describe("sidLoader cookie scope", () => {
  it("first mint under *.catalyst.example.com is wide-scope from the start", () => {
    const base = sidLoader(req("https://studio.catalyst.example.com/studio"));
    expect(base.created).toBe(true);
    const set = cookies(base);
    expect(set).toHaveLength(1);
    expect(set[0]).toContain(`sid=${base.sid}`);
    expect(set[0]).toContain("Domain=catalyst.example.com");
  });

  it("first mint elsewhere stays host-scope", () => {
    const base = sidLoader(req("https://sites.example.com/"));
    expect(base.created).toBe(true);
    const set = cookies(base);
    expect(set).toHaveLength(1);
    expect(set[0]).not.toContain("Domain=");
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

  it("a single existing sid sets nothing", () => {
    const base = sidLoader(req("https://studio.catalyst.example.com/studio", "sid=only"));
    expect(base.created).toBe(false);
    expect(cookies(base)).toHaveLength(0);
  });

  it("a split jar off the shared parent is left alone \u{2014} no domain to converge onto", () => {
    const base = sidLoader(req("https://sites.example.com/", "sid=a; sid=b"));
    expect(cookies(base)).toHaveLength(0);
  });
});
