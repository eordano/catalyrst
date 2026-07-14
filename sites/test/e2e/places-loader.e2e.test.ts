import { it, expect } from "vitest";

import { describeRequiringPg } from "./require-dep";

import { loader } from "../../packages/routes/app/routes/places";

const d = describeRequiringPg();

const GENESIS_PLAZA = "3ca25728-5b41-48f6-8e24-5cb0e3f2bb5d";

async function runLoader(url: string) {
  const request = new Request(url);
  const result: unknown = await loader({
    request,
    params: {},
    context: {},
  } as never);
  if (result && typeof result === "object" && "data" in (result as object)) {
    return (result as { data: unknown }).data as Record<string, unknown>;
  }
  return result as Record<string, unknown>;
}

d("Places route loader (SSR, temp postgres)", () => {
  it("returns the seeded places + categories with no filters", async () => {
    const payload = await runLoader("http://localhost/places");
    const places = payload.places as Array<{ id: string }>;
    const categories = payload.categories as Array<{ name: string }>;

    expect(Array.isArray(places)).toBe(true);
    expect(places.length).toBeGreaterThan(0);
    expect(places.map((p) => p.id)).toContain(GENESIS_PLAZA);

    expect(categories.length).toBeGreaterThan(0);
    expect(categories.some((c) => c.name === "music")).toBe(true);

    expect(typeof payload.sid).toBe("string");
    expect((payload.sid as string).length).toBeGreaterThan(0);
  });

  it("honors ?category= (server-side filter)", async () => {
    const payload = await runLoader("http://localhost/places?category=music");
    const places = payload.places as Array<{ categories: string[] }>;
    expect(places.length).toBeGreaterThan(0);
    for (const p of places) expect(p.categories).toContain("music");
    expect(payload.category).toBe("music");
  });

  it("honors ?search= (FTS/ILIKE) and surfaces the match", async () => {
    const payload = await runLoader("http://localhost/places?search=plaza");
    const places = payload.places as Array<{ id: string }>;
    expect(places.map((p) => p.id)).toContain(GENESIS_PLAZA);
    expect(payload.search).toBe("plaza");
  });
});
