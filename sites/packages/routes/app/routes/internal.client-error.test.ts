import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { action, normalizeReport } from "./internal.client-error";

function post(body: string): Request {
  return new Request("https://catalyst.example.com/internal/client-error", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body,
  });
}

function call(request: Request) {
  return action({ request } as unknown as Parameters<typeof action>[0]);
}

describe("normalizeReport", () => {
  it("keeps a well-formed report with clipped fields, defaults name/ts, and drops signal-less input", () => {
    const r = normalizeReport({
      message: "  boom   happened ",
      name: "TypeError",
      stack: "x".repeat(20000),
      url: "https://catalyst.example.com/marketplace",
      ua: "jsdom",
      ts: "2026-07-06T00:00:00.000Z",
    });
    expect(r).not.toBeNull();
    expect(r!.message).toBe("boom happened");
    expect(r!.name).toBe("TypeError");
    expect(r!.stack.length).toBeLessThan(20000);
    expect(r!.url).toBe("https://catalyst.example.com/marketplace");

    const bare = normalizeReport({ message: "kaboom" });
    expect(bare!.name).toBe("Error");
    expect(bare!.ts.length).toBeGreaterThan(0);

    expect(normalizeReport({ url: "https://catalyst.example.com" })).toBeNull();
    expect(normalizeReport({ message: "   ", stack: "  " })).toBeNull();
    expect(normalizeReport(null)).toBeNull();
    expect(normalizeReport("nope")).toBeNull();
  });
});

describe("action (client-error ingest)", () => {
  let errSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
  });
  afterEach(() => {
    errSpy.mockRestore();
    delete process.env.TELEMETRY_URL;
  });

  it("records a valid report with a 202 and never throws to the client", async () => {
    const res = await call(
      post(JSON.stringify({ message: "boom", name: "TypeError", stack: "at foo" })),
    );
    expect(res).toBeInstanceOf(Response);
    expect(res.status).toBe(202);
    expect(await res.json()).toEqual({ ok: true });
    expect(errSpy).toHaveBeenCalledTimes(1);
    expect(errSpy.mock.calls[0][0]).toBe("[client-error]");
    const logged = JSON.parse(errSpy.mock.calls[0][1] as string);
    expect(logged.name).toBe("TypeError");
    expect(logged.message).toBe("boom");
  });

  it("answers non-POST 405, a signal-less body 204, an oversized body 413 and bad JSON 400, recording nothing", async () => {
    const get = await action({
      request: new Request("https://catalyst.example.com/internal/client-error", { method: "GET" }),
    } as unknown as Parameters<typeof action>[0]);
    expect(get.status).toBe(405);
    expect((await call(post(JSON.stringify({ url: "https://catalyst.example.com" })))).status).toBe(204);
    expect((await call(post(JSON.stringify({ message: "x".repeat(20 * 1024) })))).status).toBe(413);
    expect((await call(post("{not json"))).status).toBe(400);
    expect(errSpy).not.toHaveBeenCalled();
  });
});
