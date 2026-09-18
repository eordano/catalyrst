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

const TRAVERSAL_EVENTS = [
  { type: "NEXT" as const },
  { type: "REVIEW" as const },
  { type: "SAVE" as const },
  { type: "CANCEL" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("editUpdateMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(editUpdateMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.general);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("general");
    }
    for (const same of ["financials", "confirm", "saving", "done"]) {
      expect(slugToState(same)).toBe(same);
    }
    expect(slugToState("save-error")).toBe("saveError");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("editUpdateMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot; saving and confirm hydrate silently (no auto-save, no double-fire); a real transition after hydration fires", async () => {
    const trackCtx = inputFor(okSave, () => {}).trackCtx;
    expect(resolveEditSnapshot({ step: "general", trackCtx, draft: DRAFT })).toBeUndefined();

    const track = vi.fn();
    const save = vi.fn(okSave);
    const saving = createActor(editUpdateMachine, {
      input: inputFor(save, track),
      snapshot: resolveEditSnapshot({ step: "saving", trackCtx, draft: DRAFT, saveUpdate: save, track }),
    }).start();
    expect(saving.getSnapshot().matches("saving")).toBe(true);
    expect(saving.getSnapshot().context.draft.updateId).toBe(DRAFT.updateId);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();
    expect(saving.getSnapshot().matches("saving")).toBe(true);

    const confirm = createActor(editUpdateMachine, {
      input: inputFor(okSave, track),
      snapshot: resolveEditSnapshot({ step: "confirm", trackCtx, draft: DRAFT, track }),
    }).start();
    expect(confirm.getSnapshot().matches("confirm")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    const financials = createActor(editUpdateMachine, {
      input: inputFor(okSave, track),
      snapshot: resolveEditSnapshot({ step: "financials", trackCtx, draft: DRAFT, track }),
    }).start();
    expect(financials.getSnapshot().matches("financials")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    financials.send({ type: "REVIEW" });
    expect(financials.getSnapshot().matches("confirm")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EDIT_EVENTS.financials);
    expect(events).toContain(EDIT_EVENTS.confirmOpen);
  });
});

describe("editUpdateMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("the funnel states are event-reachable and saving passes through NEXT, REVIEW, and SAVE", () => {
    const paths = getShortestPaths(editUpdateMachine, {
      input: inputFor(okSave, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) ends.add(p.state.value as string);
    for (const s of ["financials", "confirm", "saving"]) {
      expect(ends.has(s)).toBe(true);
    }

    const saving = paths.find((p) => (p.state.value as string) === "saving");
    expect(saving).toBeDefined();
    expect(saving!.steps.map((s) => s.event.type)).toEqual(
      expect.arrayContaining(["NEXT", "REVIEW", "SAVE"]),
    );
  });
});

describe("editUpdateMachine \u{2014} telemetry events (happy path)", () => {
  it("CANCEL from confirm returns to financials without saving; general -> financials -> confirm -> save -> done then fires the full funnel", async () => {
    const track = vi.fn();
    const save = vi.fn(okSave);
    const actor = createActor(editUpdateMachine, {
      input: inputFor(save, track),
    }).start();

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("financials")).toBe(true);
    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("confirm")).toBe(true);

    actor.send({ type: "CANCEL" });
    expect(actor.getSnapshot().matches("financials")).toBe(true);
    let events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EDIT_EVENTS.confirmOpen);
    expect(events).not.toContain(EDIT_EVENTS.saveAttempted);
    expect(save).not.toHaveBeenCalled();

    actor.send({ type: "REVIEW" });
    actor.send({ type: "SAVE" });
    await waitFor(actor, (s) => s.matches("done"));

    events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        EDIT_EVENTS.started,
        EDIT_EVENTS.financials,
        EDIT_EVENTS.confirmOpen,
        EDIT_EVENTS.saveAttempted,
        EDIT_EVENTS.saved,
      ]),
    );
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

    const r = await simulateSave({ projectId: "p1", updateId: "u1" });
    expect(r.updateId).toBe("u1");
  });
});

describe("editUpdateMachine \u{2014} save failure + retry", () => {
  it("save error -> CANCEL returns to confirm; a second failure -> RETRY recovers to done", async () => {
    const track = vi.fn();
    let calls = 0;
    const save: SaveFn = async (args) => {
      calls += 1;
      if (calls <= 2) throw new Error("governance unreachable");
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

    actor.send({ type: "CANCEL" });
    expect(actor.getSnapshot().matches("confirm")).toBe(true);

    actor.send({ type: "SAVE" });
    await waitFor(actor, (s) => s.matches("saveError"));

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("done"));
    expect(track.mock.calls.map((c) => c[0])).toContain(EDIT_EVENTS.saved);
    expect(calls).toBe(3);
  });
});
