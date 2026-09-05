import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  editUpdateMachine,
  EDIT_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveEditSnapshot,
  slugToState,
  stateToSlug,
  simulateSave,
  type EditDraft,
  type SaveFn,
  type SaveResult,
  type TrackFn,
} from "./machine";

const DRAFT: EditDraft = {
  projectId: "b783aa8f-ebf2-4792-b3eb-8dfccf369dfb",
  updateId: "f03b76f1-2314-4eb6-a6bd-06f23848d42b",
  health: "onTrack",
  introduction: "Welcome to the sixth and last grant update.",
  highlights: "### Bevy (Desktop)\n- depth-of-field imposters",
  blockers: "Without blockers",
  next_steps: "Merge Backpack -> Outfits with Catalyst persistence.",
  additional_notes: "",
  recordCount: 3,
};

const RESULT: SaveResult = { updateId: DRAFT.updateId };

const okSave: SaveFn = async () => RESULT;

function inputFor(saveUpdate: SaveFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-edit-project-update",
      variant: "wizard",
      experimentKey: "gv_update_edit_wizard",
    },
    draft: DRAFT,
    saveUpdate,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "general",
  "financials",
  "confirm",
  "saving",
  "saveError",
  "done",
]);

const TRAVERSAL_EVENTS = [
  { type: "NEXT" as const },
  { type: "REVIEW" as const },
  { type: "SAVE" as const },
  { type: "CANCEL" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("editUpdateMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(editUpdateMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.general);
    expect(slugToState(null)).toBe("general");
    expect(slugToState(undefined)).toBe("general");
    expect(slugToState("")).toBe("general");
    expect(slugToState("nope")).toBe("general");
    expect(slugToState("financials")).toBe("financials");
    expect(slugToState("confirm")).toBe("confirm");
    expect(slugToState("saving")).toBe("saving");
    expect(slugToState("save-error")).toBe("saveError");
    expect(slugToState("done")).toBe("done");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("editUpdateMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveEditSnapshot({
      step: "general",
      trackCtx: inputFor(okSave, () => {}).trackCtx,
      draft: DRAFT,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-save", async () => {
    const track = vi.fn();
    const save = vi.fn(okSave);
    const snapshot = resolveEditSnapshot({
      step: "saving",
      trackCtx: inputFor(save, track).trackCtx,
      draft: DRAFT,
      saveUpdate: save,
      track,
    });
    const actor = createActor(editUpdateMachine, {
      input: inputFor(save, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("saving")).toBe(true);
    expect(actor.getSnapshot().context.draft.updateId).toBe(DRAFT.updateId);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("saving")).toBe(true);
  });

  it("hydrating confirm does NOT re-fire its entry telemetry (no double-fire)", () => {
    const track = vi.fn();
    const snapshot = resolveEditSnapshot({
      step: "confirm",
      trackCtx: inputFor(okSave, track).trackCtx,
      draft: DRAFT,
      track,
    });
    const actor = createActor(editUpdateMachine, {
      input: inputFor(okSave, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("confirm")).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveEditSnapshot({
      step: "financials",
      trackCtx: inputFor(okSave, track).trackCtx,
      draft: DRAFT,
      track,
    });
    const actor = createActor(editUpdateMachine, {
      input: inputFor(okSave, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("financials")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("confirm")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EDIT_EVENTS.financials);
    expect(events).toContain(EDIT_EVENTS.confirmOpen);
  });
});

describe("editUpdateMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(editUpdateMachine, {
      input: inputFor(okSave, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("financials")).toBe(true);
    expect(ends.has("confirm")).toBe(true);
    expect(ends.has("saving")).toBe(true);
  });

  it("reaching saving passes through NEXT, REVIEW, and SAVE", () => {
    const paths = getShortestPaths(editUpdateMachine, {
      input: inputFor(okSave, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const saving = paths.find((p) => (p.state.value as string) === "saving");
    expect(saving).toBeDefined();
    const events = saving!.steps.map((s) => s.event.type);
    expect(events).toContain("NEXT");
    expect(events).toContain("REVIEW");
    expect(events).toContain("SAVE");
  });
});

describe("editUpdateMachine \u{2014} telemetry events (happy path)", () => {
  it("general -> financials -> confirm -> save -> done fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(editUpdateMachine, {
      input: inputFor(okSave, track),
    }).start();

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("financials")).toBe(true);

    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("confirm")).toBe(true);

    actor.send({ type: "SAVE" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EDIT_EVENTS.started);
    expect(events).toContain(EDIT_EVENTS.financials);
    expect(events).toContain(EDIT_EVENTS.confirmOpen);
    expect(events).toContain(EDIT_EVENTS.saveAttempted);
    expect(events).toContain(EDIT_EVENTS.saved);

    expect(events.indexOf(EDIT_EVENTS.saveAttempted)).toBeLessThan(
      events.indexOf(EDIT_EVENTS.saved),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === EDIT_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_update_edit_wizard",
      variant: "wizard",
    });
    const savedCall = track.mock.calls.find((c) => c[0] === EDIT_EVENTS.saved);
    expect(savedCall?.[1]).toMatchObject({ simulated: true });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("cancel from confirm returns to financials without saving", () => {
    const track = vi.fn();
    const save = vi.fn(okSave);
    const actor = createActor(editUpdateMachine, {
      input: inputFor(save, track),
    }).start();

    actor.send({ type: "NEXT" });
    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("confirm")).toBe(true);
    actor.send({ type: "CANCEL" });
    expect(actor.getSnapshot().matches("financials")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EDIT_EVENTS.confirmOpen);
    expect(events).not.toContain(EDIT_EVENTS.saveAttempted);
    expect(save).not.toHaveBeenCalled();
  });
});

describe("editUpdateMachine \u{2014} save failure + retry", () => {
  it("save error -> RETRY recovers to done", async () => {
    const track = vi.fn();
    let calls = 0;
    const save: SaveFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("governance unreachable");
      return okSave(args);
    };

    const actor = createActor(editUpdateMachine, {
      input: inputFor(save, track),
    }).start();

    actor.send({ type: "NEXT" });
    actor.send({ type: "REVIEW" });
    actor.send({ type: "SAVE" });
    await waitFor(actor, (s) => s.matches("saveError"));
    expect(actor.getSnapshot().context.error).toBe("governance unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EDIT_EVENTS.saved);
  });

  it("save error -> CANCEL returns to confirm", async () => {
    const track = vi.fn();
    const save: SaveFn = async () => {
      throw new Error("nope");
    };
    const actor = createActor(editUpdateMachine, {
      input: inputFor(save, track),
    }).start();

    actor.send({ type: "NEXT" });
    actor.send({ type: "REVIEW" });
    actor.send({ type: "SAVE" });
    await waitFor(actor, (s) => s.matches("saveError"));
    actor.send({ type: "CANCEL" });
    expect(actor.getSnapshot().matches("confirm")).toBe(true);
  });
});

describe("simulateSave", () => {
  it("resolves the edited update id (no network)", async () => {
    const r = await simulateSave({ projectId: "p1", updateId: "u1" });
    expect(r.updateId).toBe("u1");
  });
});
