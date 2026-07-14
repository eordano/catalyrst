import { describe, expect, it } from "vitest";

import { action } from "./internal.page-not-found";

function post(body: string): Request {
  return new Request("https://catalyst.example.com/internal/page-not-found", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body,
  });
}

function call(request: Request) {
  return action({ request } as unknown as Parameters<typeof action>[0]);
}

describe("internal.page-not-found action", () => {
  it("rejects non-POST", async () => {
    const res = await call(new Request("https://catalyst.example.com/internal/page-not-found"));
    expect(res.status).toBe(405);
  });

  it("swallows garbage bodies silently", async () => {
    expect((await call(post("not json"))).status).toBe(204);
    expect((await call(post(JSON.stringify({ referrer: "x" })))).status).toBe(204);
    expect((await call(post(JSON.stringify({ path: "no-leading-slash" })))).status).toBe(204);
  });

  it("accepts a well-formed spa 404 report", async () => {
    const res = await call(post(JSON.stringify({ path: "/missing-page?q=1", referrer: "https://catalyst.example.com/" })));
    expect(res.status).toBe(202);
  });
});
