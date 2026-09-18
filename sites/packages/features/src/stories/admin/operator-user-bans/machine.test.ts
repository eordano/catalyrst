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

function driveTo(actor: ReturnType<typeof createActor>, action: "ban" | "warn" | "unban", address: string, durationMs?: number | null) {
  actor.start();
  actor.send({ type: "SIGN_IN" });
  actor.send({ type: "SELECT", action, address, reason: "policy violation", durationMs });
  actor.send({ type: "REVIEW" });
  actor.send({ type: "COMMIT" });
}

describe("userBanMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(userBanMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.authGate);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("authGate");
    }
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("userBanMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step boots from initial; submitting hydrates silently; a bans deep link fires only on a real LOOKUP (with is_banned)", async () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const input = inputFor(commit, track);

    expect(
      resolveUserBanSnapshot({
        step: "authGate",
        trackCtx: input.trackCtx,
        moderator: MOD,
        activeAddresses: ACTIVE,
      }),
    ).toBeUndefined();

    const submitting = createActor(userBanMachine, {
      input,
      snapshot: resolveUserBanSnapshot({
        step: "submitting",
        trackCtx: input.trackCtx,
        moderator: MOD,
        activeAddresses: ACTIVE,
        commit,
        track,
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const bans = createActor(userBanMachine, {
      input,
      snapshot: resolveUserBanSnapshot({
        step: "bans",
        trackCtx: input.trackCtx,
        moderator: MOD,
        activeAddresses: ACTIVE,
        track,
      }),
    }).start();
    expect(bans.getSnapshot().matches("bans")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    bans.send({ type: "LOOKUP", address: BANNED, isBanned: true });
    const lookup = track.mock.calls.find((c) => c[0] === USER_BAN_EVENTS.lookup);
    expect(lookup?.[1]).toMatchObject({ is_banned: true });
  });
});

describe("userBanMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and confirm needs SIGN_IN, SELECT and REVIEW", () => {
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
    for (const s of ["bans", "action", "confirm"]) {
      expect(ends.has(s)).toBe(true);
    }

    const confirm = paths.find((p) => (p.state.value as string) === "confirm");
    expect(confirm).toBeDefined();
    const events = confirm!.steps.map((s) => s.event.type);
    expect(events).toEqual(expect.arrayContaining(["SIGN_IN", "SELECT", "REVIEW"]));
  });
});

describe("userBanMachine \u{2014} commit telemetry (happy paths)", () => {
  it("ban fires actionSelected + banCommitted with has_duration true for timed and false for permanent bans", async () => {
    const timedTrack = vi.fn();
    const timed = createActor(userBanMachine, { input: inputFor(okCommit, timedTrack) });
    driveTo(timed, "ban", CLEAN, 3600_000);
    await waitFor(timed, (s) => s.matches("done"));

    const events = timedTrack.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        USER_BAN_EVENTS.bansViewed,
        USER_BAN_EVENTS.actionSelected,
        USER_BAN_EVENTS.banCommitted,
      ]),
    );
    const timedCommit = timedTrack.mock.calls.find((c) => c[0] === USER_BAN_EVENTS.banCommitted);
    expect(timedCommit?.[1]).toMatchObject({ has_duration: true });

    const permanentTrack = vi.fn();
    const permanent = createActor(userBanMachine, { input: inputFor(okCommit, permanentTrack) });
    driveTo(permanent, "ban", CLEAN, null);
    await waitFor(permanent, (s) => s.matches("done"));
    const permanentCommit = permanentTrack.mock.calls.find(
      (c) => c[0] === USER_BAN_EVENTS.banCommitted,
    );
    expect(permanentCommit?.[1]).toMatchObject({ has_duration: false });
  });

  it("warn fires warningCommitted and unban fires unbanCommitted", async () => {
    const warnTrack = vi.fn();
    const warn = createActor(userBanMachine, { input: inputFor(okCommit, warnTrack) });
    driveTo(warn, "warn", CLEAN);
    await waitFor(warn, (s) => s.matches("done"));
    expect(warnTrack.mock.calls.map((c) => c[0])).toContain(USER_BAN_EVENTS.warningCommitted);

    const unbanTrack = vi.fn();
    const unban = createActor(userBanMachine, { input: inputFor(okCommit, unbanTrack) });
    driveTo(unban, "unban", BANNED);
    await waitFor(unban, (s) => s.matches("done"));
    expect(unbanTrack.mock.calls.map((c) => c[0])).toContain(USER_BAN_EVENTS.unbanCommitted);
  });
});

describe("userBanMachine \u{2014} faithful failure paths (server error contracts)", () => {
  it("already_banned (409) and no_active_ban (404) fire operator_user_ban_failed with the reason and return to action", async () => {
    const banTrack = vi.fn();
    const ban = createActor(userBanMachine, {
      input: {
        ...inputFor(rejectingCommit(new UserActionError("already_banned", BANNED)), banTrack),
        activeAddresses: [BANNED],
      },
    });
    driveTo(ban, "ban", BANNED, null);
    await waitFor(ban, (s) => s.matches("action"));
    const banFailed = banTrack.mock.calls.find((c) => c[0] === USER_BAN_EVENTS.failed);
    expect(banFailed?.[1]).toMatchObject({ action: "ban", reason: "already_banned" });
    expect(ban.getSnapshot().context.errorReason).toBe("already_banned");

    const unbanTrack = vi.fn();
    const unban = createActor(userBanMachine, {
      input: {
        ...inputFor(rejectingCommit(new UserActionError("no_active_ban", CLEAN)), unbanTrack),
        activeAddresses: [BANNED],
      },
    });
    driveTo(unban, "unban", CLEAN);
    await waitFor(unban, (s) => s.matches("action"));
    const unbanFailed = unbanTrack.mock.calls.find((c) => c[0] === USER_BAN_EVENTS.failed);
    expect(unbanFailed?.[1]).toMatchObject({ action: "unban", reason: "no_active_ban" });
  });
});

describe("failClosedCommit (machine default)", () => {
  it("rejects instead of faking a commit, and a wizard with no commit prop surfaces that error and stays out of done", async () => {
    await expect(
      failClosedCommit({ action: "ban", address: CLEAN, moderator: MOD, reason: "x" }),
    ).rejects.toThrow(/connect your wallet/i);

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
