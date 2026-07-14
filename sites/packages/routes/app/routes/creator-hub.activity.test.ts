import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { loader } from "./creator-hub.activity";

/*
 * The fails-closed shape. Every upstream rejects; the assertion is that the
 * payload carries NO showable `Datum` anywhere and the status is 503.
 *
 * This deliberately does not test that a button renders. The failure this
 * feature exists to prevent is a screen that renders a plausible number it
 * never read, and the only way to catch that is to break every source and prove
 * nothing showable survived.
 */

const SHOWABLE = new Set(["live", "sampled", "snapshot"]);

type Node = unknown;

/** Every `{ state }` object anywhere in the payload, at any depth. */
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
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("GET /creator-hub/activity", () => {
  it("fails closed: every upstream down yields 503 and no showable datum", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(
      new Error("network is down"),
    );

    const res = await loader(
      get("?address=0x1111111111111111111111111111111111111111") as never,
    );

    expect(res.init?.status).toBe(503);

    const payload = res.data as Record<string, unknown>;
    expect(payload.allUpstreamsDown).toBe(true);

    const datums = collectDatums(payload);
    expect(datums.length).toBeGreaterThan(0);
    const showable = datums.filter((d) => SHOWABLE.has(d.state));
    expect(showable).toEqual([]);
  });

  it("never hands a value-carrying key to a failed reading", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network is down"));

    const res = await loader(
      get("?address=0x1111111111111111111111111111111111111111") as never,
    );
    const payload = res.data as Record<string, unknown>;

    // The union has no `value` field on unavailable / no-sample / unbuilt, so a
    // `?? 0` at any call site cannot compile. Assert it at runtime too: the
    // enforcement mechanism is the *absence* of the key, not a null in it.
    for (const key of [
      "peopleInYourWorlds",
      "networkPresence",
      "worlds",
      "busiestScenes",
      "busiestWorlds",
    ]) {
      const d = payload[key] as Record<string, unknown>;
      expect(SHOWABLE.has(d.state as string)).toBe(false);
      expect(Object.hasOwn(d, "value")).toBe(false);
    }
  });

  it("renders the no-address state rather than someone else's data", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network is down"));

    const res = await loader(get() as never);
    const payload = res.data as Record<string, unknown>;

    // No DEMO_OWNER, ever. Absent an address the page scopes to nobody.
    expect(payload.address).toBeNull();
  });
});
