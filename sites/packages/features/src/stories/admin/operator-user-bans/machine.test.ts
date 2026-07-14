import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  userBanMachine,
  USER_BAN_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveUserBanSnapshot,
  slugToState,
  stateToSlug,
  failClosedCommit,
  type CommitFn,
  type TrackFn,
} from "./machine";
import { UserActionError } from "@data/lib/catalyst/admin/user-bans";

const MOD = "dcl-moderator";
const BANNED = "0x7e4b21d9f0a3c65e8b1d72f04a6c98e3b5d710a2";
const CLEAN = "0x1a7c93e02b8d465f9013a6c2e74f80d5b9a3e168";
const ACTIVE = [BANNED];

const okCommit: CommitFn = async ({ action, address }) => ({ action, address });

const rejectingCommit = (error: Error): CommitFn => () => Promise.reject(error);

function inputFor(commit: CommitFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "operator-user-bans",
      variant: "console",
      experimentKey: "op_user_bans_console",
    },
    moderator: MOD,
    activeAddresses: ACTIVE,
    commit,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "authGate",
  "bans",
  "action",
  "confirm",
  "submitting",
  "done",
]);

describe("userBanMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(userBanMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);
  });

  it("slugs are unique and round-trip via SLUG_TO_STATE", () => {
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
    }
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.authGate);
    expect(slugToState(null)).toBe("authGate");
    expect(slugToState(undefined)).toBe("authGate");
    expect(slugToState("")).toBe("authGate");
    expect(slugToState("nope")).toBe("authGate");
    expect(slugToState("action")).toBe("action");
    expect(slugToState("confirm")).toBe("confirm");
    expect(slugToState("submitting")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("userBanMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveUserBanSnapshot({
      step: "authGate",
      trackCtx: inputFor(okCommit, () => {}).trackCtx,
      moderator: MOD,
      activeAddresses: ACTIVE,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating `submitting` does NOT fire telemetry and does NOT auto-commit", async () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const snapshot = resolveUserBanSnapshot({
      step: "submitting",
      trackCtx: inputFor(commit, track).trackCtx,
      moderator: MOD,
      activeAddresses: ACTIVE,
      commit,
      track,
    });
    const actor = createActor(userBanMachine, {
      input: inputFor(commit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("deep link to `bans` does not double-fire the view event; real transitions still fire", () => {
    const track = vi.fn();
    const snapshot = resolveUserBanSnapshot({
      step: "bans",
      trackCtx: inputFor(okCommit, track).trackCtx,
      moderator: MOD,
      activeAddresses: ACTIVE,
      track,
    });
    const actor = createActor(userBanMachine, {
      input: inputFor(okCommit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("bans")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "LOOKUP", address: CLEAN, isBanned: false });
    expect(track.mock.calls.map((c) => c[0])).toContain(USER_BAN_EVENTS.lookup);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "SIGN_IN" as const },
  { type: "LOOKUP" as const, address: CLEAN, isBanned: false },
  { type: "SELECT" as const, action: "ban" as const, address: CLEAN, reason: "x" },
  { type: "REVIEW" as const },
  { type: "BACK" as const },
  { type: "CANCEL" as const },
  { type: "COMMIT" as const },
  { type: "CONTINUE" as const },
];

describe("userBanMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(userBanMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("bans")).toBe(true);
    expect(ends.has("action")).toBe(true);
    expect(ends.has("confirm")).toBe(true);
  });

  it("reaching confirm passes through SIGN_IN, SELECT and REVIEW", () => {
    const paths = getShortestPaths(userBanMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const confirm = paths.find((p) => (p.state.value as string) === "confirm");
    expect(confirm).toBeDefined();
    const events = confirm!.steps.map((s) => s.event.type);
    expect(events).toContain("SIGN_IN");
    expect(events).toContain("SELECT");
    expect(events).toContain("REVIEW");
  });
});

function driveTo(actor: ReturnType<typeof createActor>, action: "ban" | "warn" | "unban", address: string, durationMs?: number | null) {
  actor.start();
  actor.send({ type: "SIGN_IN" });
  actor.send({ type: "SELECT", action, address, reason: "policy violation", durationMs });
  actor.send({ type: "REVIEW" });
  actor.send({ type: "COMMIT" });
}

describe("userBanMachine \u{2014} commit telemetry (happy paths)", () => {
  it("ban (with duration) fires actionSelected + banCommitted(has_duration:true)", async () => {
    const track = vi.fn();
    const actor = createActor(userBanMachine, { input: inputFor(okCommit, track) });
    driveTo(actor, "ban", CLEAN, 3600_000);
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(USER_BAN_EVENTS.bansViewed);
    expect(events).toContain(USER_BAN_EVENTS.actionSelected);
    expect(events).toContain(USER_BAN_EVENTS.banCommitted);

    const committed = track.mock.calls.find((c) => c[0] === USER_BAN_EVENTS.banCommitted);
    expect(committed?.[1]).toMatchObject({ has_duration: true });
  });

  it("permanent ban reports has_duration:false", async () => {
    const track = vi.fn();
    const actor = createActor(userBanMachine, { input: inputFor(okCommit, track) });
    driveTo(actor, "ban", CLEAN, null);
    await waitFor(actor, (s) => s.matches("done"));
    const committed = track.mock.calls.find((c) => c[0] === USER_BAN_EVENTS.banCommitted);
    expect(committed?.[1]).toMatchObject({ has_duration: false });
  });

  it("warn fires warningCommitted", async () => {
    const track = vi.fn();
    const actor = createActor(userBanMachine, { input: inputFor(okCommit, track) });
    driveTo(actor, "warn", CLEAN);
    await waitFor(actor, (s) => s.matches("done"));
    expect(track.mock.calls.map((c) => c[0])).toContain(USER_BAN_EVENTS.warningCommitted);
  });

  it("unban fires unbanCommitted", async () => {
    const track = vi.fn();
    const actor = createActor(userBanMachine, { input: inputFor(okCommit, track) });
    driveTo(actor, "unban", BANNED);
    await waitFor(actor, (s) => s.matches("done"));
    expect(track.mock.calls.map((c) => c[0])).toContain(USER_BAN_EVENTS.unbanCommitted);
  });

  it("LOOKUP fires operator_user_ban_lookup with is_banned", () => {
    const track = vi.fn();
    const actor = createActor(userBanMachine, { input: inputFor(okCommit, track) }).start();
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "LOOKUP", address: BANNED, isBanned: true });
    const call = track.mock.calls.find((c) => c[0] === USER_BAN_EVENTS.lookup);
    expect(call?.[1]).toMatchObject({ is_banned: true });
  });
});

describe("userBanMachine \u{2014} faithful failure paths (server error contracts)", () => {
  it("banning an already-banned address -> 409, fires operator_user_ban_failed{already_banned} and returns to action", async () => {
    const track = vi.fn();
    const commit = rejectingCommit(new UserActionError("already_banned", BANNED));
    const actor = createActor(userBanMachine, {
      input: { ...inputFor(commit, track), activeAddresses: [BANNED] },
    });
    driveTo(actor, "ban", BANNED, null);
    await waitFor(actor, (s) => s.matches("action"));

    const failed = track.mock.calls.find((c) => c[0] === USER_BAN_EVENTS.failed);
    expect(failed?.[1]).toMatchObject({ action: "ban", reason: "already_banned" });
    expect(actor.getSnapshot().context.errorReason).toBe("already_banned");
  });

  it("unbanning an address with no active ban -> 404, fires operator_user_ban_failed{no_active_ban}", async () => {
    const track = vi.fn();
    const commit = rejectingCommit(new UserActionError("no_active_ban", CLEAN));
    const actor = createActor(userBanMachine, {
      input: { ...inputFor(commit, track), activeAddresses: [BANNED] },
    });
    driveTo(actor, "unban", CLEAN);
    await waitFor(actor, (s) => s.matches("action"));

    const failed = track.mock.calls.find((c) => c[0] === USER_BAN_EVENTS.failed);
    expect(failed?.[1]).toMatchObject({ action: "unban", reason: "no_active_ban" });
  });
});

describe("failClosedCommit (machine default)", () => {
  it("rejects instead of faking a commit when no wallet was wired in", async () => {
    await expect(
      failClosedCommit({ action: "ban", address: CLEAN, moderator: MOD, reason: "x" }),
    ).rejects.toThrow(/connect your wallet/i);
  });

  it("a wizard with no commit prop surfaces the error and stays out of done", async () => {
    const track = vi.fn();
    const actor = createActor(userBanMachine, {
      input: {
        trackCtx: inputFor(okCommit, track).trackCtx,
        moderator: MOD,
        activeAddresses: ACTIVE,
        track,
      },
    });
    driveTo(actor, "ban", CLEAN, null);
    await waitFor(actor, (s) => s.matches("action"));
    expect(actor.getSnapshot().context.error).toMatch(/connect your wallet/i);
  });
});
