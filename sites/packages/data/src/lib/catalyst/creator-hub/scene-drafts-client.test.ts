import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../auth/session", () => ({
  getIdentity: () => ({ ephemeral: {}, authChain: [] }),
}));
vi.mock("../../auth/signer", () => ({
  signRequest: async () => ({ headers: { "x-signed": "1" } }),
}));

import { fetchServerDraft, forgetServerDraftVersions, pushServerDraft } from "./scene-drafts-client";

type Call = { url: string; method: string; body: unknown };

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

let calls: Call[];
let script: Array<(call: Call) => Response>;

beforeEach(() => {
  calls = [];
  script = [];
  forgetServerDraftVersions();
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      const method = (init?.method ?? "GET").toUpperCase();
      const body = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
      const call = { url, method, body };
      calls.push(call);
      const next = script.shift();
      if (!next) throw new Error(`unexpected fetch ${method} ${url}`);
      return next(call);
    }),
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const blob = { composite: '{"version":1}', title: "Untitled scene" };

describe("pushServerDraft: first save without a 404 pre-flight", () => {
  it("learns the version from the list endpoint, creates at baseVersion 0, then re-uses the version returned by the PUT instead of listing again", async () => {
    script.push(() => jsonResponse(200, { drafts: [] }));
    script.push(() => jsonResponse(200, { ok: true, meta: { id: "untitled-scene", version: 1 } }));
    expect(await pushServerDraft("untitled-scene", blob)).toBe(true);
    expect(calls.map((c) => `${c.method} ${c.url}`)).toEqual([
      "GET /api/creator-hub/drafts",
      "PUT /api/creator-hub/drafts/untitled-scene",
    ]);
    expect(calls[1]!.body).toMatchObject({ baseVersion: 0, title: "Untitled scene" });

    script.push(() => jsonResponse(200, { ok: true, meta: { id: "untitled-scene", version: 2 } }));
    expect(await pushServerDraft("untitled-scene", blob)).toBe(true);
    expect(calls.map((c) => `${c.method} ${c.url}`)).toEqual([
      "GET /api/creator-hub/drafts",
      "PUT /api/creator-hub/drafts/untitled-scene",
      "PUT /api/creator-hub/drafts/untitled-scene",
    ]);
    expect(calls[2]!.body).toMatchObject({ baseVersion: 1 });
  });

  it("retries once with the server version on 409", async () => {
    script.push(() => jsonResponse(200, { drafts: [] }));
    script.push(() => jsonResponse(409, { ok: false, conflict: true, server: { version: 3 } }));
    script.push(() => jsonResponse(200, { ok: true, meta: { id: "s", version: 4 } }));
    expect(await pushServerDraft("s", blob)).toBe(true);
    expect(calls[1]!.body).toMatchObject({ baseVersion: 0 });
    expect(calls[2]!.body).toMatchObject({ baseVersion: 3 });
  });

  it("uses the listed version for an existing draft, reports false on 503, and forgets the cached version", async () => {
    script.push(() =>
      jsonResponse(200, { drafts: [{ id: "s", version: 4, updatedAt: 1, title: "s" }] }),
    );
    script.push(() => jsonResponse(503, { error: "drafts store write failed" }));
    expect(await pushServerDraft("s", blob)).toBe(false);
    expect(calls[1]!.body).toMatchObject({ baseVersion: 4 });
    script.push(() =>
      jsonResponse(200, { drafts: [{ id: "s", version: 4, updatedAt: 1, title: "s" }] }),
    );
    script.push(() => jsonResponse(200, { ok: true, meta: { id: "s", version: 5 } }));
    expect(await pushServerDraft("s", blob)).toBe(true);
    expect(calls[3]!.body).toMatchObject({ baseVersion: 4 });
    expect(calls.map((c) => `${c.method} ${c.url}`)).toEqual([
      "GET /api/creator-hub/drafts",
      "PUT /api/creator-hub/drafts/s",
      "GET /api/creator-hub/drafts",
      "PUT /api/creator-hub/drafts/s",
    ]);
  });
});


it("treats an absent optional server copy as an empty response instead of a failed resource", async () => {
  script.push(()=>new Response(null,{status:204}));
  expect(await fetchServerDraft("new-scene")).toBeNull();
  expect(calls.map(call=>call.url)).toEqual(["/api/creator-hub/drafts/new-scene?optional=1"]);
});
