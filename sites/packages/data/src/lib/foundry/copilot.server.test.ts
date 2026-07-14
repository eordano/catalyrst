import { describe, expect, it } from "vitest";

import { doorFunnelFrom, parseDoorProbeStatus, stuckFromSessionStatus } from "./copilot.server";

// The shape /session/status actually returned while the gateway was down
// (observed 2026-08-17): session id -> retry state with attempt + message.
const STUCK = {
  ses_a: {
    type: "retry",
    attempt: 17,
    message: "Cannot connect to API: Unable to connect.",
    next: 1786972765259,
  },
};

describe("stuckFromSessionStatus", () => {
  it("reports the stuck session with its attempt count and message", () => {
    expect(stuckFromSessionStatus(STUCK)).toEqual({
      attempts: 17,
      message: "Cannot connect to API: Unable to connect.",
    });
  });

  it("reports the worst session when several are retrying", () => {
    const body = {
      ...STUCK,
      ses_b: { type: "retry", attempt: 3, message: "also failing" },
    };
    expect(stuckFromSessionStatus(body)?.attempts).toBe(17);
  });

  it("ignores a first retry — one attempt is a blip, not an outage", () => {
    expect(
      stuckFromSessionStatus({ ses_a: { type: "retry", attempt: 1, message: "x" } }),
    ).toBeNull();
  });

  it("is null for an empty map, non-retry states, and junk", () => {
    expect(stuckFromSessionStatus({})).toBeNull();
    expect(stuckFromSessionStatus({ ses_a: { type: "busy" } })).toBeNull();
    expect(stuckFromSessionStatus(null)).toBeNull();
    expect(stuckFromSessionStatus("nope")).toBeNull();
    expect(
      stuckFromSessionStatus({ ses_a: { type: "retry", attempt: "17" } }),
    ).toBeNull();
  });
});

describe("doorFunnelFrom", () => {
  const t0 = 1_787_000_000_000;
  const mk = (id: string, title: string, created: number) => ({ id, title, time: { created } });

  it("counts only door-titled sessions inside the window and joins replies", () => {
    const sessions = [
      mk("ses_a", "nora — from the site", t0 + 1000),
      mk("ses_b", "colda — from the site", t0 + 2000),
      mk("ses_c", "pipeline smoke", t0 + 3000),
      mk("ses_d", "old — from the site", t0 - 5000),
    ];
    const f = doorFunnelFrom(sessions, new Set(["ses_a", "ses_c"]), t0);
    expect(f.minted).toBe(2);
    expect(f.replied).toBe(1);
    expect(f.lastMintedAt).toBe(new Date(t0 + 2000).toISOString());
  });

  it("is all-zero with a null timestamp when nothing came through the door", () => {
    const f = doorFunnelFrom([mk("ses_x", "unrelated", t0 + 1)], new Set(), t0);
    expect(f).toEqual({ minted: 0, replied: 0, lastMintedAt: null });
  });

  it("skips malformed rows instead of counting them", () => {
    const f = doorFunnelFrom(
      [{ title: "ghost — from the site" }, { id: "ses_y", title: 42 as unknown as string }],
      new Set(),
      0,
    );
    expect(f.minted).toBe(0);
  });
});

describe("parseDoorProbeStatus", () => {
  it("accepts the probe's shape and normalizes steps", () => {
    const raw = JSON.stringify({
      ok: false,
      at: "2026-08-22T07:18:45.250Z",
      steps: [{ name: "recover-form", ok: false, detail: "no code input" }, { bogus: true }],
    });
    const s = parseDoorProbeStatus(raw);
    expect(s?.ok).toBe(false);
    expect(s?.steps).toEqual([{ name: "recover-form", ok: false, detail: "no code input" }]);
  });

  it("returns null for junk, not a fabricated verdict", () => {
    expect(parseDoorProbeStatus("not json")).toBeNull();
    expect(parseDoorProbeStatus(JSON.stringify({ ok: "yes", at: 3, steps: {} }))).toBeNull();
  });
});
