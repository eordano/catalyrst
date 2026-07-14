import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  permissionsMachine,
  PERMS_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolvePermissionsSnapshot,
  slugToState,
  stateToSlug,
  simulateCommit,
  type CommitFn,
  type CommitResult,
  type TrackFn,
} from "./machine";

const RESULT: CommitResult = { aclWritten: true, addresses: 0 };

const okCommit: CommitFn = async () => RESULT;

function inputFor(commit: CommitFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "creator-hub-world-permissions",
      variant: "wizard",
      experimentKey: "ch_world_perms_wizard",
    },
    commit,
    track,
  };
}

const VALID_ADDR = "0x4a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d";
const BAD_ADDR = "0xnothex";

const EXPECTED_STATES = new Set([
  "access",
  "invite",
  "password",
  "collaborators",
  "addingCollaborator",
  "confirming",
  "finishing",
  "complete",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "START_INVITE" as const },
  { type: "SUBMIT_INVITE" as const, channel: "wallet" as const },
  { type: "OPEN_PASSWORD" as const },
  { type: "SET_PASSWORD" as const },
  { type: "ADD" as const },
  { type: "VALIDATE" as const, address: VALID_ADDR },
  { type: "CANCEL" as const },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

const serializeState = (snapshot: unknown) => {
  const s = snapshot as {
    value: unknown;
    context: { candidate?: string; addressError?: string };
  };
  return JSON.stringify({
    value: s.value,
    candidate: s.context.candidate ?? null,
    addressError: s.context.addressError ?? null,
  });
};

describe("permissionsMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(permissionsMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.access);
    expect(slugToState(null)).toBe("access");
    expect(slugToState(undefined)).toBe("access");
    expect(slugToState("")).toBe("access");
    expect(slugToState("nope")).toBe("access");
    expect(slugToState("invite")).toBe("invite");
    expect(slugToState("add-collaborator")).toBe("addingCollaborator");
    expect(slugToState("confirm")).toBe("confirming");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("permissionsMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolvePermissionsSnapshot({
      step: "access",
      trackCtx: inputFor(okCommit, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating confirm does NOT fire telemetry and does NOT auto-commit", async () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const snapshot = resolvePermissionsSnapshot({
      step: "confirming",
      trackCtx: inputFor(commit, track).trackCtx,
      commit,
      track,
    });
    const actor = createActor(permissionsMachine, {
      input: inputFor(commit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("confirming")).toBe(true);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolvePermissionsSnapshot({
      step: "addingCollaborator",
      trackCtx: inputFor(okCommit, track).trackCtx,
      track,
    });
    const actor = createActor(permissionsMachine, {
      input: inputFor(okCommit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("addingCollaborator")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "VALIDATE", address: VALID_ADDR });
    expect(actor.getSnapshot().matches("collaborators")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(
      PERMS_EVENTS.collaboratorValidated,
    );
  });
});

describe("permissionsMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(permissionsMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
      serializeState,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("invite")).toBe(true);
    expect(ends.has("password")).toBe(true);
    expect(ends.has("collaborators")).toBe(true);
    expect(ends.has("addingCollaborator")).toBe(true);
    expect(ends.has("confirming")).toBe(true);
  });

  it("reaching confirming passes through START_INVITE and CONFIRM", () => {
    const paths = getShortestPaths(permissionsMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
      serializeState,
    });
    const confirming = paths.find((p) => (p.state.value as string) === "confirming");
    expect(confirming).toBeDefined();
    const events = confirming!.steps.map((s) => s.event.type);
    expect(events).toContain("CONFIRM");
  });
});

describe("permissionsMachine \u{2014} telemetry events (happy path)", () => {
  it("access -> invite -> collaborators -> confirm -> complete fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(permissionsMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "START_INVITE" });
    expect(actor.getSnapshot().matches("invite")).toBe(true);

    actor.send({ type: "SUBMIT_INVITE", channel: "community" });
    expect(actor.getSnapshot().matches("collaborators")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("complete"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(PERMS_EVENTS.started);
    expect(events).toContain(PERMS_EVENTS.inviteSubmitted);
    expect(events).toContain(PERMS_EVENTS.confirmReached);
    expect(events).toContain(PERMS_EVENTS.completed);

    expect(events.indexOf(PERMS_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(PERMS_EVENTS.completed),
    );

    const inviteCall = track.mock.calls.find(
      (c) => c[0] === PERMS_EVENTS.inviteSubmitted,
    );
    expect(inviteCall?.[1]).toMatchObject({ channel: "community" });

    const startedCall = track.mock.calls.find((c) => c[0] === PERMS_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "ch_world_perms_wizard",
      variant: "wizard",
    });
  });

  it("commits the ACL EXACTLY ONCE on the way to complete (no double-write)", async () => {
    const commit = vi.fn(okCommit);
    const actor = createActor(permissionsMachine, {
      input: inputFor(commit, vi.fn()),
    }).start();

    actor.send({ type: "START_INVITE" });
    actor.send({ type: "SUBMIT_INVITE", channel: "community" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("complete"));

    expect(commit).toHaveBeenCalledTimes(1);
  });

  it("password path fires password_set + access_type_set", () => {
    const track = vi.fn();
    const actor = createActor(permissionsMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "OPEN_PASSWORD" });
    expect(actor.getSnapshot().matches("password")).toBe(true);
    actor.send({ type: "SET_PASSWORD" });
    expect(actor.getSnapshot().matches("access")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(PERMS_EVENTS.passwordSet);
    expect(events).toContain(PERMS_EVENTS.accessTypeSet);
  });
});

describe("permissionsMachine \u{2014} add-collaborator validation", () => {
  it("a valid 0x address is accepted and appended", () => {
    const track = vi.fn();
    const actor = createActor(permissionsMachine, {
      input: { ...inputFor(okCommit, track), collaborators: [] },
    }).start();

    actor.send({ type: "START_INVITE" });
    actor.send({ type: "SUBMIT_INVITE", channel: "wallet" });
    actor.send({ type: "ADD" });
    expect(actor.getSnapshot().matches("addingCollaborator")).toBe(true);

    actor.send({ type: "VALIDATE", address: VALID_ADDR });
    expect(actor.getSnapshot().matches("collaborators")).toBe(true);
    expect(actor.getSnapshot().context.collaborators).toContain(VALID_ADDR);

    const okCall = track.mock.calls.find(
      (c) => c[0] === PERMS_EVENTS.collaboratorValidated,
    );
    expect(okCall?.[1]).toMatchObject({ valid: true });
  });

  it("an invalid address stays in the dialog and fires the guardrail metric", () => {
    const track = vi.fn();
    const actor = createActor(permissionsMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "START_INVITE" });
    actor.send({ type: "SUBMIT_INVITE", channel: "wallet" });
    actor.send({ type: "ADD" });
    actor.send({ type: "VALIDATE", address: BAD_ADDR });

    expect(actor.getSnapshot().matches("addingCollaborator")).toBe(true);
    expect(actor.getSnapshot().context.addressError).toBeTruthy();
    expect(actor.getSnapshot().context.collaborators).not.toContain(BAD_ADDR);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(PERMS_EVENTS.invalidAddress);
    const failCall = track.mock.calls.find(
      (c) => c[0] === PERMS_EVENTS.collaboratorValidated,
    );
    expect(failCall?.[1]).toMatchObject({ valid: false });
  });
});

describe("permissionsMachine \u{2014} commit failure + retry", () => {
  it("commit error -> RETRY recovers to complete", async () => {
    const track = vi.fn();
    let calls = 0;
    const commit: CommitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("worlds-content-server unreachable");
      return okCommit(args);
    };

    const actor = createActor(permissionsMachine, {
      input: inputFor(commit, track),
    }).start();

    actor.send({ type: "START_INVITE" });
    actor.send({ type: "SUBMIT_INVITE", channel: "wallet" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe(
      "worlds-content-server unreachable",
    );

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("complete"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(PERMS_EVENTS.completed);
  });
});

describe("simulateCommit", () => {
  it("resolves an ACL-written result counting collaborators + marks itself stub:true (no network)", async () => {
    const r = await simulateCommit({
      accessType: "allow-list",
      collaborators: [VALID_ADDR],
    });
    expect(r.aclWritten).toBe(true);
    expect(r.addresses).toBe(1);
    expect(r.stub).toBe(true);
  });
});

describe("permissionsMachine \u{2014} stub telemetry contract", () => {
  async function completedEventProps(commit: CommitFn) {
    const track = vi.fn();
    const actor = createActor(permissionsMachine, {
      input: inputFor(commit, track),
    }).start();

    actor.send({ type: "START_INVITE" });
    actor.send({ type: "SUBMIT_INVITE", channel: "wallet" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("complete"));

    const call = track.mock.calls.find((c) => c[0] === PERMS_EVENTS.completed);
    expect(call).toBeDefined();
    return call![1] as Record<string, unknown>;
  }

  it("default simulateCommit emits stub:true on the completed event", async () => {
    const props = await completedEventProps(simulateCommit);
    expect(props).toMatchObject({ stub: true });
  });

  it("an injected real CommitFn (stub:false) emits stub:false \u{2014} the seam is honest without machine changes", async () => {
    const realCommit: CommitFn = async ({ collaborators }) => ({
      aclWritten: true,
      addresses: collaborators.length,
      stub: false,
    });
    const props = await completedEventProps(realCommit);
    expect(props).toMatchObject({ stub: false });
    expect(props.stub).not.toBe(true);
  });

  it("a CommitFn that omits stub defaults to stub:false (treated as real)", async () => {
    const impliedRealCommit: CommitFn = async ({ collaborators }) => ({
      aclWritten: true,
      addresses: collaborators.length,
    });
    const props = await completedEventProps(impliedRealCommit);
    expect(props).toMatchObject({ stub: false });
  });
});

describe("permissionsMachine \u{2014} minimal-clicks contract", () => {
  const TARGET_EVENTS = 3;

  it(`reaches the commit in <= ${TARGET_EVENTS} events and commits exactly once`, async () => {
    const paths = getShortestPaths(permissionsMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
      serializeState,
    });
    const toConfirming = paths.filter((p) => (p.state.value as string) === "confirming");
    expect(toConfirming.length).toBeGreaterThan(0);
    const minWeight = Math.min(...toConfirming.map((p) => p.weight));
    expect(minWeight).toBeLessThanOrEqual(TARGET_EVENTS);

    const best = toConfirming.reduce((a, b) => (b.weight < a.weight ? b : a));
    const events = best.steps
      .map((s) => s.event.type as string)
      .filter((t) => t !== "xstate.init");
    expect(events).toEqual(["START_INVITE", "SUBMIT_INVITE", "CONFIRM"]);

    const commit = vi.fn(okCommit);
    const actor = createActor(permissionsMachine, {
      input: inputFor(commit, vi.fn()),
    }).start();
    actor.send({ type: "START_INVITE" });
    actor.send({ type: "SUBMIT_INVITE", channel: "wallet" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("complete"));
    expect(commit).toHaveBeenCalledTimes(1);
  });
});
