import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { StoryMeta } from "./context";
import { bucket, resolveAssignment } from "./assign";
import {
  experimentActive,
  getForcedFlags,
  getRuntimeFlags,
  resetRuntimeFlagCache,
  resolveFlag,
} from "./flags";

const BASE = "https://telemetry.example.com";

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function makeStory(
  variants: { id: string; weight: number; flags?: Record<string, unknown> }[],
  key = "gv_vote_flow",
): StoryMeta {
  return {
    id: "governance-vote",
    status: "draft",
    owner: "qa",
    hypothesis: { statement: "x", because: "y" },
    metric: { primary: "gv_vote_completed_rate", guardrails: [] },
    experiment: {
      key,
      unit: "session",
      variants: variants.map((v) => ({
        id: v.id,
        weight: v.weight,
        flags: v.flags ?? {},
      })),
    },
    decision: { rule: "ship if primary up" },
  };
}

const VOTE_STORY = makeStory([
  { id: "control", weight: 50, flags: { guided: false } },
  { id: "guided", weight: 50, flags: { guided: true } },
]);

function requestWithSid(sid: string): Request {
  return new Request("https://sites.example.com/", {
    headers: { cookie: `sid=${sid}` },
  });
}

function hashedVariant(sid: string): string {
  return bucket(sid, VOTE_STORY.experiment.key, VOTE_STORY.experiment.variants).id;
}

const fetchMock = vi.fn();

function routeMock(forcedBody: unknown, experimentsBody: unknown = {}) {
  fetchMock.mockImplementation((url: string) =>
    Promise.resolve(jsonResponse(url.includes("/dash/flags") ? forcedBody : experimentsBody)),
  );
}

const notJson = () =>
  new Response("<html>not json</html>", { status: 200, headers: { "content-type": "text/html" } });

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
  fetchMock.mockReset();
  resetRuntimeFlagCache();
  process.env.TELEMETRY_URL = BASE;
});

afterEach(() => {
  vi.unstubAllGlobals();
  delete process.env.TELEMETRY_URL;
});

describe("getRuntimeFlags (per-experiment override endpoint)", () => {
  it("returns null and never fetches when TELEMETRY_URL is unset", async () => {
    delete process.env.TELEMETRY_URL;
    expect(await getRuntimeFlags("gv_vote_flow")).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("GETs /dash/experiments?key=<expKey> as JSON, trimming the base slash and encoding the key", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({}));
    await getRuntimeFlags("gv_vote_flow");
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe(`${BASE}/dash/experiments?key=gv_vote_flow`);
    expect((init.headers as Record<string, string>).Accept).toBe("application/json");

    resetRuntimeFlagCache();
    process.env.TELEMETRY_URL = `${BASE}/`;
    fetchMock.mockResolvedValueOnce(jsonResponse({}));
    await getRuntimeFlags("ns/exp key");
    expect(fetchMock.mock.calls[1]?.[0]).toBe(`${BASE}/dash/experiments?key=ns%2Fexp%20key`);
  });

  it("maps kill, forced variant and flags rows; {} and a no-op row are no override", async () => {
    const cases: [unknown, unknown][] = [
      [{ killed: true, variant: null, flags: {} }, { killed: true }],
      [{ killed: false, variant: "guided", flags: {} }, { variant: "guided" }],
      [{ killed: false, variant: null, flags: { guided: true } }, { flags: { guided: true } }],
      [{}, null],
      [{ killed: false, variant: null, flags: {} }, null],
    ];
    for (const [row, expected] of cases) {
      resetRuntimeFlagCache();
      fetchMock.mockResolvedValueOnce(jsonResponse(row));
      expect(await getRuntimeFlags("gv_vote_flow")).toEqual(expected);
    }
  });

  it("fails open to null on a non-2xx, a non-JSON body, or an unreachable host", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ killed: true }, 500));
    expect(await getRuntimeFlags("gv_vote_flow")).toBeNull();
    resetRuntimeFlagCache();
    fetchMock.mockResolvedValueOnce(notJson());
    expect(await getRuntimeFlags("gv_vote_flow")).toBeNull();
    resetRuntimeFlagCache();
    fetchMock.mockRejectedValueOnce(new Error("ECONNREFUSED"));
    expect(await getRuntimeFlags("gv_vote_flow")).toBeNull();
  });
});

describe("resolveAssignment honors overrides from the endpoint", () => {
  it("a kill switch pins to the default variant, or to the variant it names", async () => {
    routeMock({ flags: {} }, { killed: true, variant: null, flags: {} });
    for (const sid of ["a", "b", "c", "d"]) {
      const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
      expect(a.variant).toBe("control");
      expect(a.flags).toEqual({ guided: false });
    }
    resetRuntimeFlagCache();
    routeMock({ flags: {} }, { killed: true, variant: "guided", flags: {} });
    const pinned = await resolveAssignment(requestWithSid("sid-1"), VOTE_STORY);
    expect(pinned.variant).toBe("guided");
    expect(pinned.flags).toEqual({ guided: true });
  });

  it("a forced variant overrides the hash for every sid", async () => {
    routeMock({ flags: {} }, { killed: false, variant: "guided", flags: {} });
    for (const sid of ["a", "b", "c", "d"]) {
      const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
      expect(a.variant).toBe("guided");
      expect(a.flags).toEqual({ guided: true });
    }
  });

  it("a flags override overlays onto the locally-hashed variant; no override row keeps the hash", async () => {
    routeMock({ flags: {} }, { killed: false, variant: null, flags: { banner: "x" } });
    const overlaid = await resolveAssignment(requestWithSid("sid-overlay"), VOTE_STORY);
    expect(overlaid.variant).toBe(hashedVariant("sid-overlay"));
    expect(overlaid.flags).toMatchObject({ banner: "x" });

    resetRuntimeFlagCache();
    routeMock({ flags: {} }, {});
    const plain = await resolveAssignment(requestWithSid("sid-fallback"), VOTE_STORY);
    expect(plain.variant).toBe(hashedVariant("sid-fallback"));
  });
});

describe("getForcedFlags (override-merged /dash/flags endpoint)", () => {
  it("returns null and never fetches when TELEMETRY_URL is unset", async () => {
    delete process.env.TELEMETRY_URL;
    expect(await getForcedFlags()).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("GETs /dash/flags as JSON (base slash trimmed) and coerces the merged flags map", async () => {
    process.env.TELEMETRY_URL = `${BASE}/`;
    fetchMock.mockResolvedValueOnce(
      jsonResponse({
        flags: {
          new_nav: {
            value: true,
            variant: "guided",
            upstream_value: false,
            overridden: true,
            override_state: "forced",
          },
          legacy: {
            value: false,
            variant: null,
            upstream_value: false,
            overridden: false,
            override_state: null,
          },
        },
      }),
    );
    expect(await getForcedFlags()).toEqual({
      new_nav: { value: true, variant: "guided", overridden: true },
      legacy: { value: false, overridden: false },
    });
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe(`${BASE}/dash/flags`);
    expect((init.headers as Record<string, string>).Accept).toBe("application/json");
  });

  it("fails open to null without a flags object, on a non-2xx, a non-JSON body, or an unreachable host", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ config: {}, observed: [] }));
    expect(await getForcedFlags()).toBeNull();
    resetRuntimeFlagCache();
    fetchMock.mockResolvedValueOnce(jsonResponse({ flags: {} }, 500));
    expect(await getForcedFlags()).toBeNull();
    resetRuntimeFlagCache();
    fetchMock.mockResolvedValueOnce(notJson());
    expect(await getForcedFlags()).toBeNull();
    resetRuntimeFlagCache();
    fetchMock.mockRejectedValueOnce(new Error("ECONNREFUSED"));
    expect(await getForcedFlags()).toBeNull();
  });
});

describe("resolveFlag honors a forced flag over the caller default", () => {
  it("a forced flag wins in both directions", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ flags: { g: { value: true, overridden: true } } }));
    expect(await resolveFlag("g", false)).toBe(true);
    resetRuntimeFlagCache();
    fetchMock.mockResolvedValue(jsonResponse({ flags: { g: { value: false, overridden: true } } }));
    expect(await resolveFlag("g", true)).toBe(false);
  });

  it("keeps the caller default for upstream-only, unknown, service-down and unconfigured flags", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ flags: { g: { value: true, overridden: false } } }));
    expect(await resolveFlag("g", false)).toBe(false);
    expect(await resolveFlag("missing", true)).toBe(true);
    resetRuntimeFlagCache();
    fetchMock.mockRejectedValue(new Error("ECONNREFUSED"));
    expect(await resolveFlag("g", true)).toBe(true);
    expect(await resolveFlag("h", false)).toBe(false);
    resetRuntimeFlagCache();
    fetchMock.mockReset();
    delete process.env.TELEMETRY_URL;
    expect(await resolveFlag("g", true)).toBe(true);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("resolveAssignment honors dashboard forced flags", () => {
  it("a forced-off flag pins every sid to the default variant, or to the story variant it names", async () => {
    routeMock({ flags: { gv_vote_flow: { value: false, overridden: true } } });
    for (const sid of ["a", "b", "c", "d"]) {
      const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
      expect(a.variant).toBe("control");
      expect(a.flags).toEqual({ guided: false });
    }
    resetRuntimeFlagCache();
    routeMock({ flags: { gv_vote_flow: { value: false, variant: "guided", overridden: true } } });
    const named = await resolveAssignment(requestWithSid("sid-1"), VOTE_STORY);
    expect(named.variant).toBe("guided");
    expect(named.flags).toEqual({ guided: true });
  });

  it("a forced variant pins every sid and wins over the experiment override and the kill switch", async () => {
    const forced = { flags: { gv_vote_flow: { value: true, variant: "guided", overridden: true } } };
    routeMock(forced);
    for (const sid of ["a", "b", "c", "d"]) {
      const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
      expect(a.variant).toBe("guided");
      expect(a.flags).toEqual({ guided: true });
    }
    resetRuntimeFlagCache();
    routeMock(forced, { killed: false, variant: "control", flags: {} });
    expect((await resolveAssignment(requestWithSid("sid-1"), VOTE_STORY)).variant).toBe("guided");
    resetRuntimeFlagCache();
    routeMock(forced, { killed: true, variant: null, flags: {} });
    expect((await resolveAssignment(requestWithSid("sid-1"), VOTE_STORY)).variant).toBe("guided");
  });

  it("a forced variant outside the story returns it verbatim with no flags", async () => {
    routeMock({ flags: { gv_vote_flow: { value: true, variant: "beta", overridden: true } } });
    const a = await resolveAssignment(requestWithSid("sid-1"), VOTE_STORY);
    expect(a.variant).toBe("beta");
    expect(a.flags).toEqual({});
  });

  it("a forced-on flag neutralizes a kill switch back to the local hash", async () => {
    routeMock(
      { flags: { gv_vote_flow: { value: true, overridden: true } } },
      { killed: true, variant: null, flags: {} },
    );
    const sids = Array.from({ length: 32 }, (_, i) => `kill-${i}`);
    const guidedSid = sids.find((s) => hashedVariant(s) === "guided");
    expect(guidedSid).toBeDefined();
    const a = await resolveAssignment(requestWithSid(guidedSid!), VOTE_STORY);
    expect(a.variant).toBe("guided");
  });

  it("non-overridden or unrelated entries keep the computed assignment, and the user key reaches /dash/flags", async () => {
    routeMock({ flags: { gv_vote_flow: { value: true, variant: "guided", overridden: false } } });
    expect((await resolveAssignment(requestWithSid("sid-upstream-only"), VOTE_STORY)).variant).toBe(
      hashedVariant("sid-upstream-only"),
    );
    resetRuntimeFlagCache();
    routeMock({ flags: { anything: { value: true, overridden: true } } });
    expect((await resolveAssignment(requestWithSid("sid-determinism"), VOTE_STORY)).variant).toBe(
      hashedVariant("sid-determinism"),
    );
    const urls = fetchMock.mock.calls.map((c) => c[0] as string);
    expect(urls.some((u) => u.includes("/dash/flags") && u.includes("user=sid-determinism"))).toBe(true);
  });

  it("degrades one layer at a time: /dash/flags down, everything down, or no TELEMETRY_URL", async () => {
    fetchMock.mockImplementation((url: string) =>
      url.includes("/dash/flags")
        ? Promise.reject(new Error("ECONNREFUSED"))
        : Promise.resolve(jsonResponse({ killed: false, variant: "guided", flags: {} })),
    );
    expect((await resolveAssignment(requestWithSid("sid-1"), VOTE_STORY)).variant).toBe("guided");

    resetRuntimeFlagCache();
    fetchMock.mockRejectedValue(new Error("ECONNREFUSED"));
    expect((await resolveAssignment(requestWithSid("sid-down"), VOTE_STORY)).variant).toBe(
      hashedVariant("sid-down"),
    );

    resetRuntimeFlagCache();
    fetchMock.mockReset();
    delete process.env.TELEMETRY_URL;
    expect((await resolveAssignment(requestWithSid("sid-no-env"), VOTE_STORY)).variant).toBe(
      hashedVariant("sid-no-env"),
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("experimentActive (runtime activation of a draft)", () => {
  it("envActive short-circuits to true without fetching; without it an unset TELEMETRY_URL stays draft", async () => {
    delete process.env.TELEMETRY_URL;
    expect(await experimentActive("gv_vote_flow", { envActive: true })).toBe(true);
    expect(await experimentActive("gv_vote_flow")).toBe(false);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("stays draft on {} or a no-op row; activates on a flags payload or a variant pin", async () => {
    const cases: [unknown, boolean][] = [
      [{}, false],
      [{ killed: false, variant: null, flags: {} }, false],
      [{ killed: false, variant: null, flags: { active: true } }, true],
      [{ killed: false, variant: "guided", flags: {} }, true],
    ];
    for (const [row, expected] of cases) {
      resetRuntimeFlagCache();
      fetchMock.mockResolvedValueOnce(jsonResponse(row));
      expect(await experimentActive("gv_vote_flow")).toBe(expected);
    }
  });

  it("a kill row deactivates even with a pinned variant, and errors fail closed to draft", async () => {
    for (const row of [
      { killed: true, variant: null, flags: {} },
      { killed: true, variant: "guided", flags: {} },
    ]) {
      resetRuntimeFlagCache();
      fetchMock.mockResolvedValueOnce(jsonResponse(row));
      expect(await experimentActive("gv_vote_flow")).toBe(false);
    }
    resetRuntimeFlagCache();
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ killed: false, variant: null, flags: { active: true } }, 500),
    );
    expect(await experimentActive("gv_vote_flow")).toBe(false);
    resetRuntimeFlagCache();
    fetchMock.mockRejectedValueOnce(new Error("ECONNREFUSED"));
    expect(await experimentActive("gv_vote_flow")).toBe(false);
  });

  it("envActive plus a kill row: active, and resolveAssignment still pins base", async () => {
    routeMock({ flags: {} }, { killed: true, variant: null, flags: {} });
    expect(await experimentActive("gv_vote_flow", { envActive: true })).toBe(true);
    const a = await resolveAssignment(requestWithSid("sid-env"), VOTE_STORY);
    expect(a.variant).toBe("control");
    expect(a.flags).toEqual({ guided: false });
  });

  it("rides the resolveAssignment fetch for the same (key, user) and refetches per user", async () => {
    routeMock({ flags: {} }, { killed: false, variant: null, flags: { active: true } });
    await resolveAssignment("sid-cache", VOTE_STORY, { user: "user-1" });
    expect(await experimentActive("gv_vote_flow", { user: "user-1" })).toBe(true);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(await experimentActive("gv_vote_flow", { user: "user-2" })).toBe(true);
    expect(fetchMock).toHaveBeenCalledTimes(3);
    const lastUrl = fetchMock.mock.calls[2][0] as string;
    expect(lastUrl).toContain("/dash/experiments");
    expect(lastUrl).toContain("user=user-2");
  });
});
