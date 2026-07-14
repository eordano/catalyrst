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

describe("getRuntimeFlags (per-experiment override endpoint)", () => {
  const fetchMock = vi.fn();

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

  it("returns null and never fetches when TELEMETRY_URL is unset", async () => {
    delete process.env.TELEMETRY_URL;
    expect(await getRuntimeFlags("gv_vote_flow")).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("GETs /dash/experiments?key=<expKey> with Accept: application/json", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({}));
    await getRuntimeFlags("gv_vote_flow");
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [
      string,
      RequestInit,
    ];
    expect(url).toBe(`${BASE}/dash/experiments?key=gv_vote_flow`);
    const headers = init.headers as Record<string, string>;
    expect(headers.Accept).toBe("application/json");
  });

  it("trims a trailing slash on TELEMETRY_URL and url-encodes the key", async () => {
    process.env.TELEMETRY_URL = `${BASE}/`;
    fetchMock.mockResolvedValueOnce(jsonResponse({}));
    await getRuntimeFlags("ns/exp key");
    const [url] = fetchMock.mock.calls[0] as unknown as [string];
    expect(url).toBe(`${BASE}/dash/experiments?key=ns%2Fexp%20key`);
  });

  it("maps a kill row to { killed: true }", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ killed: true, variant: null, flags: {} }),
    );
    expect(await getRuntimeFlags("gv_vote_flow")).toEqual({ killed: true });
  });

  it("maps forced_variant (JSON `variant`) to a forced variant", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ killed: false, variant: "guided", flags: {} }),
    );
    expect(await getRuntimeFlags("gv_vote_flow")).toEqual({ variant: "guided" });
  });

  it("maps a non-empty flags object to a flags override", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ killed: false, variant: null, flags: { guided: true } }),
    );
    expect(await getRuntimeFlags("gv_vote_flow")).toEqual({
      flags: { guided: true },
    });
  });

  it("treats {} (no override row) as no override", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({}));
    expect(await getRuntimeFlags("gv_vote_flow")).toBeNull();
  });

  it("treats a no-op row (killed:false, no variant, empty flags) as no override", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ killed: false, variant: null, flags: {} }),
    );
    expect(await getRuntimeFlags("gv_vote_flow")).toBeNull();
  });

  it("returns null on a non-2xx response", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ killed: true }, 500));
    expect(await getRuntimeFlags("gv_vote_flow")).toBeNull();
  });

  it("returns null when the body is not JSON", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response("<html>not json</html>", {
        status: 200,
        headers: { "content-type": "text/html" },
      }),
    );
    expect(await getRuntimeFlags("gv_vote_flow")).toBeNull();
  });

  it("returns null when fetch rejects (host unreachable)", async () => {
    fetchMock.mockRejectedValueOnce(new Error("ECONNREFUSED"));
    expect(await getRuntimeFlags("gv_vote_flow")).toBeNull();
  });
});

describe("resolveAssignment honors overrides from the endpoint", () => {
  const fetchMock = vi.fn();

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

  it("a kill switch pins to the default (control) variant instead of the hash", async () => {
    fetchMock.mockImplementation(() =>
      Promise.resolve(jsonResponse({ killed: true, variant: null, flags: {} })),
    );
    for (const sid of ["a", "b", "c", "d"]) {
      const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
      expect(a.variant).toBe("control");
      expect(a.flags).toEqual({ guided: false });
    }
  });

  it("a kill switch can pin to an explicit variant", async () => {
    fetchMock.mockImplementation(() =>
      Promise.resolve(jsonResponse({ killed: true, variant: "guided", flags: {} })),
    );
    const a = await resolveAssignment(requestWithSid("sid-1"), VOTE_STORY);
    expect(a.variant).toBe("guided");
    expect(a.flags).toEqual({ guided: true });
  });

  it("a forced variant overrides the hash for every sid", async () => {
    fetchMock.mockImplementation(() =>
      Promise.resolve(jsonResponse({ killed: false, variant: "guided", flags: {} })),
    );
    for (const sid of ["a", "b", "c", "d"]) {
      const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
      expect(a.variant).toBe("guided");
      expect(a.flags).toEqual({ guided: true });
    }
  });

  it("a flags override overlays onto the locally-hashed variant", async () => {
    fetchMock.mockImplementation(() =>
      Promise.resolve(
        jsonResponse({ killed: false, variant: null, flags: { banner: "x" } }),
      ),
    );
    const sid = "sid-overlay";
    const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
    const hashed = bucket(
      sid,
      VOTE_STORY.experiment.key,
      VOTE_STORY.experiment.variants,
    );
    expect(a.variant).toBe(hashed.id);
    expect(a.flags).toMatchObject({ banner: "x" });
  });

  it("no override row ({}) falls back to the deterministic local hash", async () => {
    fetchMock.mockResolvedValue(jsonResponse({}));
    const sid = "sid-fallback";
    const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
    const hashed = bucket(
      sid,
      VOTE_STORY.experiment.key,
      VOTE_STORY.experiment.variants,
    );
    expect(a.variant).toBe(hashed.id);
  });
});

describe("getForcedFlags (override-merged /dash/flags endpoint)", () => {
  const fetchMock = vi.fn();

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

  it("returns null and never fetches when TELEMETRY_URL is unset", async () => {
    delete process.env.TELEMETRY_URL;
    expect(await getForcedFlags()).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("GETs /dash/flags with Accept: application/json", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ flags: {} }));
    await getForcedFlags();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [
      string,
      RequestInit,
    ];
    expect(url).toBe(`${BASE}/dash/flags`);
    expect((init.headers as Record<string, string>).Accept).toBe(
      "application/json",
    );
  });

  it("trims a trailing slash on TELEMETRY_URL", async () => {
    process.env.TELEMETRY_URL = `${BASE}/`;
    fetchMock.mockResolvedValueOnce(jsonResponse({ flags: {} }));
    await getForcedFlags();
    const [url] = fetchMock.mock.calls[0] as unknown as [string];
    expect(url).toBe(`${BASE}/dash/flags`);
  });

  it("coerces the merged flags map (value / variant / overridden)", async () => {
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
  });

  it("returns null when the payload has no flags object", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ config: {}, observed: [] }));
    expect(await getForcedFlags()).toBeNull();
  });

  it("returns null on a non-2xx response (fail-open)", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ flags: {} }, 500));
    expect(await getForcedFlags()).toBeNull();
  });

  it("returns null when the body is not JSON (fail-open)", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response("<html>not json</html>", {
        status: 200,
        headers: { "content-type": "text/html" },
      }),
    );
    expect(await getForcedFlags()).toBeNull();
  });

  it("returns null when fetch rejects (host unreachable, fail-open)", async () => {
    fetchMock.mockRejectedValueOnce(new Error("ECONNREFUSED"));
    expect(await getForcedFlags()).toBeNull();
  });
});

describe("resolveFlag honors a forced flag over the caller default", () => {
  const fetchMock = vi.fn();

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

  it("a forced-on flag overrides a false upstream default", async () => {
    fetchMock.mockResolvedValue(
      jsonResponse({ flags: { g: { value: true, overridden: true } } }),
    );
    expect(await resolveFlag("g", false)).toBe(true);
  });

  it("a forced-off flag overrides a true upstream default", async () => {
    fetchMock.mockResolvedValue(
      jsonResponse({ flags: { g: { value: false, overridden: true } } }),
    );
    expect(await resolveFlag("g", true)).toBe(false);
  });

  it("a non-overridden (upstream-only) flag keeps the caller default", async () => {
    fetchMock.mockResolvedValue(
      jsonResponse({ flags: { g: { value: true, overridden: false } } }),
    );
    expect(await resolveFlag("g", false)).toBe(false);
  });

  it("an unknown flag keeps the caller default", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ flags: {} }));
    expect(await resolveFlag("missing", true)).toBe(true);
  });

  it("service-down falls back to the caller default (fail-open)", async () => {
    fetchMock.mockRejectedValue(new Error("ECONNREFUSED"));
    expect(await resolveFlag("g", true)).toBe(true);
    expect(await resolveFlag("h", false)).toBe(false);
  });

  it("TELEMETRY_URL unset falls back without fetching", async () => {
    delete process.env.TELEMETRY_URL;
    expect(await resolveFlag("g", true)).toBe(true);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("resolveAssignment honors dashboard forced flags", () => {
  const fetchMock = vi.fn();

  function routeMock(forcedBody: unknown, experimentsBody: unknown = {}) {
    fetchMock.mockImplementation((url: string) =>
      Promise.resolve(
        jsonResponse(url.includes("/dash/flags") ? forcedBody : experimentsBody),
      ),
    );
  }

  function hashedVariant(sid: string): string {
    return bucket(sid, VOTE_STORY.experiment.key, VOTE_STORY.experiment.variants)
      .id;
  }

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

  it("a forced-off flag pins every sid to the default variant", async () => {
    routeMock({
      flags: { gv_vote_flow: { value: false, overridden: true } },
    });
    for (const sid of ["a", "b", "c", "d"]) {
      const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
      expect(a.variant).toBe("control");
      expect(a.flags).toEqual({ guided: false });
    }
  });

  it("a forced-off flag pins to its variant when it matches the story", async () => {
    routeMock({
      flags: {
        gv_vote_flow: { value: false, variant: "guided", overridden: true },
      },
    });
    const a = await resolveAssignment(requestWithSid("sid-1"), VOTE_STORY);
    expect(a.variant).toBe("guided");
    expect(a.flags).toEqual({ guided: true });
  });

  it("a forced variant pins every sid to that variant", async () => {
    routeMock({
      flags: {
        gv_vote_flow: { value: true, variant: "guided", overridden: true },
      },
    });
    for (const sid of ["a", "b", "c", "d"]) {
      const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
      expect(a.variant).toBe("guided");
      expect(a.flags).toEqual({ guided: true });
    }
  });

  it("a forced variant wins over the experiment-override variant", async () => {
    routeMock(
      {
        flags: {
          gv_vote_flow: { value: true, variant: "guided", overridden: true },
        },
      },
      { killed: false, variant: "control", flags: {} },
    );
    const a = await resolveAssignment(requestWithSid("sid-1"), VOTE_STORY);
    expect(a.variant).toBe("guided");
  });

  it("a forced variant wins over a kill switch", async () => {
    routeMock(
      {
        flags: {
          gv_vote_flow: { value: true, variant: "guided", overridden: true },
        },
      },
      { killed: true, variant: null, flags: {} },
    );
    const a = await resolveAssignment(requestWithSid("sid-1"), VOTE_STORY);
    expect(a.variant).toBe("guided");
  });

  it("a forced variant outside the story returns it verbatim", async () => {
    routeMock({
      flags: {
        gv_vote_flow: { value: true, variant: "beta", overridden: true },
      },
    });
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

  it("a non-overridden entry keeps the computed assignment", async () => {
    routeMock({
      flags: {
        gv_vote_flow: { value: true, variant: "guided", overridden: false },
      },
    });
    const sid = "sid-upstream-only";
    const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
    expect(a.variant).toBe(hashedVariant(sid));
  });

  it("a forced flag for another name does not perturb the assignment", async () => {
    routeMock({ flags: { anything: { value: true, overridden: true } } });
    const sid = "sid-determinism";
    const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
    expect(a.variant).toBe(hashedVariant(sid));
  });

  it("passes the user key through to /dash/flags", async () => {
    routeMock({ flags: {} });
    await resolveAssignment(requestWithSid("sid-user"), VOTE_STORY);
    const urls = fetchMock.mock.calls.map((c) => c[0] as string);
    expect(
      urls.some((u) => u.includes("/dash/flags") && u.includes("user=sid-user")),
    ).toBe(true);
  });

  it("service-down on /dash/flags degrades to the experiment-override path", async () => {
    fetchMock.mockImplementation((url: string) => {
      if (url.includes("/dash/flags")) {
        return Promise.reject(new Error("ECONNREFUSED"));
      }
      return Promise.resolve(
        jsonResponse({ killed: false, variant: "guided", flags: {} }),
      );
    });
    const a = await resolveAssignment(requestWithSid("sid-1"), VOTE_STORY);
    expect(a.variant).toBe("guided");
  });

  it("service fully down degrades to the deterministic local hash", async () => {
    fetchMock.mockRejectedValue(new Error("ECONNREFUSED"));
    const sid = "sid-down";
    const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
    expect(a.variant).toBe(hashedVariant(sid));
  });

  it("TELEMETRY_URL unset stays on the local hash without fetching", async () => {
    delete process.env.TELEMETRY_URL;
    const sid = "sid-no-env";
    const a = await resolveAssignment(requestWithSid(sid), VOTE_STORY);
    expect(a.variant).toBe(hashedVariant(sid));
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("experimentActive (runtime activation of a draft)", () => {
  const fetchMock = vi.fn();

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

  it("envActive short-circuits to true without fetching (even with TELEMETRY_URL unset)", async () => {
    delete process.env.TELEMETRY_URL;
    expect(await experimentActive("gv_vote_flow", { envActive: true })).toBe(true);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("TELEMETRY_URL unset with no envActive stays draft without fetching", async () => {
    delete process.env.TELEMETRY_URL;
    expect(await experimentActive("gv_vote_flow")).toBe(false);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("no override row ({}) stays draft", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({}));
    expect(await experimentActive("gv_vote_flow")).toBe(false);
  });

  it("a no-op row (UI save with nothing set) cannot activate a draft", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ killed: false, variant: null, flags: {} }),
    );
    expect(await experimentActive("gv_vote_flow")).toBe(false);
  });

  it("a non-empty flags payload activates (bucketed rollout)", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ killed: false, variant: null, flags: { active: true } }),
    );
    expect(await experimentActive("gv_vote_flow")).toBe(true);
  });

  it("a variant pin activates (100% rollout)", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ killed: false, variant: "guided", flags: {} }),
    );
    expect(await experimentActive("gv_vote_flow")).toBe(true);
  });

  it("a kill row deactivates, even with a pinned variant", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ killed: true, variant: null, flags: {} }),
    );
    expect(await experimentActive("gv_vote_flow")).toBe(false);
    resetRuntimeFlagCache();
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ killed: true, variant: "guided", flags: {} }),
    );
    expect(await experimentActive("gv_vote_flow")).toBe(false);
  });

  it("fails closed to draft on HTTP errors and network failure", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ killed: false, variant: null, flags: { active: true } }, 500),
    );
    expect(await experimentActive("gv_vote_flow")).toBe(false);
    resetRuntimeFlagCache();
    fetchMock.mockRejectedValueOnce(new Error("ECONNREFUSED"));
    expect(await experimentActive("gv_vote_flow")).toBe(false);
  });

  it("envActive plus a kill row: active, and resolveAssignment still pins base", async () => {
    fetchMock.mockImplementation((url: string) =>
      Promise.resolve(
        jsonResponse(
          url.includes("/dash/flags")
            ? { flags: {} }
            : { killed: true, variant: null, flags: {} },
        ),
      ),
    );
    expect(await experimentActive("gv_vote_flow", { envActive: true })).toBe(true);
    const a = await resolveAssignment(requestWithSid("sid-env"), VOTE_STORY);
    expect(a.variant).toBe("control");
    expect(a.flags).toEqual({ guided: false });
  });

  it("rides the resolveAssignment fetch for the same (key, user); refetches per user", async () => {
    fetchMock.mockImplementation((url: string) =>
      Promise.resolve(
        jsonResponse(
          url.includes("/dash/flags")
            ? { flags: {} }
            : { killed: false, variant: null, flags: { active: true } },
        ),
      ),
    );
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
