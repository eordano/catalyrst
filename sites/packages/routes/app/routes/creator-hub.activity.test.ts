import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { loader } from "./creator-hub.activity";

const SHOWABLE = new Set(["live", "sampled", "snapshot"]);

type Node = unknown;

function collectDatums(node: Node, out: { state: string }[] = []): { state: string }[] {
  if (Array.isArray(node)) {
    for (const item of node) collectDatums(item, out);
    return out;
  }
  if (node && typeof node === "object") {
    const rec = node as Record<string, unknown>;
    if (typeof rec.state === "string") out.push({ state: rec.state });
    for (const value of Object.values(rec)) collectDatums(value, out);
  }
  return out;
}

function get(search = "") {
  return {
    request: new Request(`https://sites.test/creator-hub/activity${search}`),
    params: {},
    context: {} as never,
  };
}

beforeEach(() => {
  vi.restoreAllMocks();
  vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network is down"));
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("GET /creator-hub/activity", () => {
  it("fails closed: every upstream down yields 503, no showable datum and no value key on a failed reading", async () => {
    const res = await loader(
      get("?address=0x1111111111111111111111111111111111111111") as never,
    );
    expect(res.init?.status).toBe(503);

    const payload = res.data as Record<string, unknown>;
    expect(payload.allUpstreamsDown).toBe(true);

    const datums = collectDatums(payload);
    expect(datums.length).toBeGreaterThan(0);
    expect(datums.filter((d) => SHOWABLE.has(d.state))).toEqual([]);

    for (const key of [
      "peopleInYourWorlds",
      "networkPresence",
      "worlds",
      "busiestScenes",
      "busiestWorlds",
    ]) {
      const d = payload[key] as Record<string, unknown>;
      expect(SHOWABLE.has(d.state as string), key).toBe(false);
      expect(Object.hasOwn(d, "value"), key).toBe(false);
    }
  });

  it("renders the no-address state rather than someone else's data", async () => {
    const res = await loader(get() as never);
    expect((res.data as Record<string, unknown>).address).toBeNull();
  });
});
