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
  it("keeps a well-formed report and clips oversized fields", () => {
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
    expect(r!.stack.length).toBe(8000);
    expect(r!.url).toBe("https://catalyst.example.com/marketplace");
  });

  it("drops a report with no message and no stack", () => {
    expect(normalizeReport({ url: "https://catalyst.example.com" })).toBeNull();
    expect(normalizeReport({ message: "   ", stack: "  " })).toBeNull();
    expect(normalizeReport(null)).toBeNull();
    expect(normalizeReport("nope")).toBeNull();
  });

  it("defaults name/ts when absent but a message exists", () => {
    const r = normalizeReport({ message: "kaboom" });
    expect(r!.name).toBe("Error");
    expect(typeof r!.ts).toBe("string");
    expect(r!.ts.length).toBeGreaterThan(0);
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

  it("records a valid report and returns {ok:true} 202", async () => {
    const res = await call(
      post(JSON.stringify({ message: "boom", name: "TypeError", stack: "at foo" })),
    );
    expect(res.status).toBe(202);
    expect(await res.json()).toEqual({ ok: true });
    expect(errSpy).toHaveBeenCalledTimes(1);
    expect(errSpy.mock.calls[0][0]).toBe("[client-error]");
    const logged = JSON.parse(errSpy.mock.calls[0][1] as string);
    expect(logged.name).toBe("TypeError");
    expect(logged.message).toBe("boom");
  });

  it("rejects non-POST with 405 and records nothing", async () => {
    const res = await action({
      request: new Request("https://catalyst.example.com/internal/client-error", { method: "GET" }),
    } as unknown as Parameters<typeof action>[0]);
    expect(res.status).toBe(405);
    expect(errSpy).not.toHaveBeenCalled();
  });

  it("accepts an empty/no-signal body with 204 and records nothing", async () => {
    const res = await call(post(JSON.stringify({ url: "https://catalyst.example.com" })));
    expect(res.status).toBe(204);
    expect(errSpy).not.toHaveBeenCalled();
  });

  it("rejects an oversized body with 413 and records nothing", async () => {
    const huge = JSON.stringify({ message: "x".repeat(20 * 1024) });
    const res = await call(post(huge));
    expect(res.status).toBe(413);
    expect(errSpy).not.toHaveBeenCalled();
  });

  it("rejects invalid JSON with 400 and records nothing", async () => {
    const res = await call(post("{not json"));
    expect(res.status).toBe(400);
    expect(errSpy).not.toHaveBeenCalled();
  });

  it("never throws to the client", async () => {
    await expect(call(post(JSON.stringify({ message: "boom" })))).resolves.toBeInstanceOf(
      Response,
    );
  });
});
