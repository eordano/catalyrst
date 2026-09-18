import { describe, expect, it, vi } from "vitest";
import { createElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";

vi.mock("@ui/components/Modal", () => ({
  default: (props: { children?: ReactNode }) => props.children,
}));
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import CreateProjectView from "@ui/creatorhub/workflows/CreateProjectView";
import {
  createProjectMachine,
  COLD_RESUMABLE_STATES,
  CREATE_PROJECT_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  SCAFFOLD_FILES,
  resolveCreateProjectSnapshot,
  slugToState,
  stateToSlug,
  slugifyProjectName,
  type ScaffoldFn,
  type ScaffoldResult,
  type TrackFn,
} from "./machine";

const RESULT: ScaffoldResult = { files: SCAFFOLD_FILES };

const okScaffold: ScaffoldFn = async () => RESULT;

function inputFor(scaffold: ScaffoldFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "creator-hub-create-project",
      variant: "wizard",
      experimentKey: "ch_create_project_wizard",
    },
    scaffold,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "naming",
  "templating",
  "scaffolding",
  "created",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "SET_NAME" as const, name: "My Scene", valid: true },
  { type: "SET_NAME" as const, name: "Taken Scene", valid: false },
  { type: "SELECT_TEMPLATE" as const, template: "empty" },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("createProjectMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, and falls back to the first step (including the retired path step)", () => {
    const machineStates = new Set(Object.keys(createProjectMachine.states));
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

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.naming);
    for (const bad of [null, undefined, "", "nope", "path"]) {
      expect(slugToState(bad)).toBe("naming");
    }
    expect(slugToState("template")).toBe("templating");
    expect(slugToState("scaffold")).toBe("scaffolding");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("createProjectMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot; run-scoped steps (scaffold/created/error) are NOT cold-resumable and fall back to naming", async () => {
    const track = vi.fn();
    const scaffold = vi.fn(okScaffold);
    const input = inputFor(scaffold, track);

    expect(resolveCreateProjectSnapshot({ step: "naming", trackCtx: input.trackCtx })).toBeUndefined();

    for (const step of ["scaffolding", "created", "error"] as const) {
      expect(COLD_RESUMABLE_STATES).not.toContain(step);
      expect(
        resolveCreateProjectSnapshot({
          step,
          trackCtx: input.trackCtx,
          scaffold,
          track,
          name: "My Awesome Scene",
          template: "art-gallery",
        }),
      ).toBeUndefined();
    }

    const actor = createActor(createProjectMachine, { input, snapshot: undefined }).start();
    expect(actor.getSnapshot().matches("naming")).toBe(true);
    expect(actor.getSnapshot().context.result).toBeUndefined();

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(scaffold).not.toHaveBeenCalled();
  });

  it("hydrating templating seeds slug + path silently; SELECT_TEMPLATE then fires telemetry and reaches created", async () => {
    const track = vi.fn();
    const scaffold = vi.fn(okScaffold);
    const snapshot = resolveCreateProjectSnapshot({
      step: "templating",
      trackCtx: inputFor(scaffold, track).trackCtx,
      scaffold,
      track,
      name: "Neon Market",
    });
    const actor = createActor(createProjectMachine, {
      input: inputFor(scaffold, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("templating")).toBe(true);
    expect(actor.getSnapshot().context.name).toBe("Neon Market");
    expect(actor.getSnapshot().context.projectSlug).toBe("neon-market");
    expect(actor.getSnapshot().context.path).toBe("neon-market");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(scaffold).not.toHaveBeenCalled();

    actor.send({ type: "SELECT_TEMPLATE", template: "empty" });
    expect(track.mock.calls.map((c) => c[0])).toContain(CREATE_PROJECT_EVENTS.templateSelected);
    await waitFor(actor, (s) => s.matches("created"));
    expect(actor.getSnapshot().context.projectSlug).toBe("neon-market");
  });
});

describe("createProjectMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and scaffolding needs a valid name + a template", () => {
    const paths = getShortestPaths(createProjectMachine, {
      input: inputFor(okScaffold, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    for (const s of ["naming", "templating", "scaffolding"]) {
      expect(ends.has(s)).toBe(true);
    }

    const scaffolding = paths.find((p) => (p.state.value as string) === "scaffolding");
    expect(scaffolding).toBeDefined();
    const events = scaffolding!.steps.map((s) => s.event.type);
    expect(events).toEqual(expect.arrayContaining(["SET_NAME", "SELECT_TEMPLATE"]));
  });
});

describe("createProjectMachine \u{2014} telemetry events (happy path)", () => {
  it("name -> template -> scaffold -> created fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(createProjectMachine, {
      input: inputFor(okScaffold, track),
    }).start();

    actor.send({ type: "SET_NAME", name: "Neon Market", valid: true });
    expect(actor.getSnapshot().matches("templating")).toBe(true);
    expect(actor.getSnapshot().context.name).toBe("Neon Market");
    expect(actor.getSnapshot().context.path).toBe("neon-market");

    actor.send({ type: "SELECT_TEMPLATE", template: "art-gallery" });
    await waitFor(actor, (s) => s.matches("created"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        CREATE_PROJECT_EVENTS.started,
        CREATE_PROJECT_EVENTS.nameSet,
        CREATE_PROJECT_EVENTS.pathSet,
        CREATE_PROJECT_EVENTS.templateSelected,
        CREATE_PROJECT_EVENTS.scaffolding,
        CREATE_PROJECT_EVENTS.completed,
      ]),
    );
    expect(events.indexOf(CREATE_PROJECT_EVENTS.started)).toBeLessThan(
      events.indexOf(CREATE_PROJECT_EVENTS.completed),
    );
    expect(events).not.toContain(CREATE_PROJECT_EVENTS.pathInvalid);

    const startedCall = track.mock.calls.find((c) => c[0] === CREATE_PROJECT_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "ch_create_project_wizard",
      variant: "wizard",
    });

    const completedCall = track.mock.calls.find((c) => c[0] === CREATE_PROJECT_EVENTS.completed);
    expect(completedCall?.[1]).toMatchObject({ written: false, template: "art-gallery" });
    expect(completedCall?.[1].files).toEqual(SCAFFOLD_FILES.map((f) => f.name));
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("createProjectMachine \u{2014} invalid name guardrail + project slug", () => {
  it("an invalid SET_NAME stays in naming with the given (or default) error and no slug; a valid name clears it, sets the slug, and carries it to created", async () => {
    const track = vi.fn();
    const actor = createActor(createProjectMachine, {
      input: inputFor(okScaffold, track),
    }).start();

    actor.send({ type: "SET_NAME", name: "Taken", valid: false, error: "Name already in use" });
    const invalid = actor.getSnapshot();
    expect(invalid.matches("naming")).toBe(true);
    expect(invalid.context.name).toBe("Taken");
    expect(invalid.context.pathError).toBe("Name already in use");
    expect(invalid.context.projectSlug).toBeUndefined();
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CREATE_PROJECT_EVENTS.pathInvalid);
    expect(events).not.toContain(CREATE_PROJECT_EVENTS.pathSet);

    actor.send({ type: "SET_NAME", name: "X", valid: false });
    expect(actor.getSnapshot().matches("naming")).toBe(true);
    expect(actor.getSnapshot().context.pathError).toBeTruthy();

    actor.send({ type: "SET_NAME", name: "My Awesome Scene", valid: true });
    const mid = actor.getSnapshot();
    expect(mid.matches("templating")).toBe(true);
    expect(mid.context.pathError).toBeUndefined();
    expect(mid.context.projectSlug).toBe("my-awesome-scene");
    expect(mid.context.path).toBe("my-awesome-scene");
    expect(track.mock.calls.map((c) => c[0])).toContain(CREATE_PROJECT_EVENTS.pathSet);

    actor.send({ type: "SELECT_TEMPLATE", template: "empty" });
    await waitFor(actor, (s) => s.matches("created"));
    expect(actor.getSnapshot().context.projectSlug).toBe("my-awesome-scene");

    expect(slugifyProjectName("My Awesome Scene")).toBe("my-awesome-scene");
    expect(slugifyProjectName("  Neon   Market!! ")).toBe("neon-market");
    expect(slugifyProjectName("***")).toBe("new-scene");
    expect(slugifyProjectName("")).toBe("new-scene");
  });
});

describe("createProjectMachine \u{2014} preselected template skips templating", () => {
  it("a valid SET_NAME goes straight to scaffolding (preselected:true); an invalid SET_NAME still stays in naming", async () => {
    const track = vi.fn();
    const actor = createActor(createProjectMachine, {
      input: { ...inputFor(okScaffold, track), template: "tower-defense" },
    }).start();

    actor.send({ type: "SET_NAME", name: "Tower Defense", valid: true });
    expect(actor.getSnapshot().matches("templating")).toBe(false);
    await waitFor(actor, (s) => s.matches("created"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CREATE_PROJECT_EVENTS.templateSelected);
    const selectedCall = track.mock.calls.find((c) => c[0] === CREATE_PROJECT_EVENTS.templateSelected);
    expect(selectedCall?.[1]).toMatchObject({
      template: "tower-defense",
      preselected: true,
    });
    expect(actor.getSnapshot().context.template).toBe("tower-defense");

    const invalid = createActor(createProjectMachine, {
      input: { ...inputFor(okScaffold, vi.fn()), template: "memory-game" },
    }).start();
    invalid.send({ type: "SET_NAME", name: "Taken", valid: false });
    expect(invalid.getSnapshot().matches("naming")).toBe(true);
  });
});

describe("createProjectMachine \u{2014} scaffold failure + retry", () => {
  it("scaffold error -> RETRY recovers to created", async () => {
    const track = vi.fn();
    let calls = 0;
    const scaffold: ScaffoldFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("disk full");
      return okScaffold(args);
    };

    const actor = createActor(createProjectMachine, {
      input: inputFor(scaffold, track),
    }).start();

    actor.send({ type: "SET_NAME", name: "Retry Scene", valid: true });
    actor.send({ type: "SELECT_TEMPLATE", template: "empty" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("disk full");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("created"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CREATE_PROJECT_EVENTS.completed);
  });
});

describe("CreateProjectView \u{2014} created screen create->edit seam", () => {
  const result: ScaffoldResult = {
    files: SCAFFOLD_FILES,
    via: "directory",
    folder: "my-awesome-scene",
    written: true,
  };

  it("offers one primary 'Open in editor' deep link + 'Go to My Scenes', notes a missing wallet without gating, and never fabricates file writes", () => {
    const signedIn = renderToStaticMarkup(
      createElement(CreateProjectView, {
        view: "created",
        name: "My Awesome Scene",
        projectSlug: "my-awesome-scene",
        signedIn: true,
        result,
      }),
    );
    expect(signedIn).toContain("Open in editor");
    expect(signedIn).toContain("/creator-hub/scene-editor?source=local");
    expect(signedIn).toContain("project=my-awesome-scene");
    expect(signedIn).toContain('href="/create/scenes"');
    expect(signedIn).toContain("Go to My Scenes");
    expect((signedIn.match(/create-project-wizard__btn--primary/g) ?? []).length).toBe(1);

    const signedOut = renderToStaticMarkup(
      createElement(CreateProjectView, {
        view: "created",
        name: "Scene",
        projectSlug: "scene",
        signedIn: false,
        result,
      }),
    );
    expect(signedOut).toMatch(/publishing this scene later needs a connected wallet/i);
    expect(signedOut).toContain("Open in editor");

    const noResult = renderToStaticMarkup(
      createElement(CreateProjectView, {
        view: "created",
        name: "Scene",
        projectSlug: "scene",
        signedIn: true,
      }),
    );
    expect(noResult).not.toContain("0 files");
    expect(noResult).not.toMatch(/Wrote/);
    expect(noResult).toMatch(/Your scene is ready/);
    expect(noResult).toContain("Open in editor");
  });

  it("surfaces the name error inline (role=alert) and offers [Choose folder again] as the primary action after a dismissed picker", () => {
    const naming = renderToStaticMarkup(
      createElement(CreateProjectView, {
        view: "naming",
        pathError: "A scene with that name already exists.",
      }),
    );
    expect(naming).toContain("A scene with that name already exists.");
    expect(naming).toContain('role="alert"');

    const error = renderToStaticMarkup(
      createElement(CreateProjectView, {
        view: "error",
        error: "Scene creation canceled \u{2014} no folder was chosen.",
      }),
    );
    expect(error).toContain("Scene creation was cancelled \u{2014} no folder was chosen.");
    expect(error).toContain("Choose folder again");
    expect(error).toContain("Retry");
    expect((error.match(/btn btn--primary/g) ?? []).length).toBe(1);
  });
});

describe("createProjectMachine \u{2014} minimal-clicks contract", () => {
  const TARGET_EVENTS = 2;

  it(`reaches scaffolding (scene-create trigger) in <= ${TARGET_EVENTS} events + no pathing state`, () => {
    const paths = getShortestPaths(createProjectMachine, {
      input: inputFor(okScaffold, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const toScaffolding = paths.filter((p) => (p.state.value as string) === "scaffolding");
    expect(toScaffolding.length).toBeGreaterThan(0);
    const minWeight = Math.min(...toScaffolding.map((p) => p.weight));
    expect(minWeight).toBeLessThanOrEqual(TARGET_EVENTS);

    const best = toScaffolding.reduce((a, b) => (b.weight < a.weight ? b : a));
    const events = best.steps
      .map((s) => s.event.type as string)
      .filter((t) => t !== "xstate.init");
    expect(events).toEqual(["SET_NAME", "SELECT_TEMPLATE"]);

    expect(Object.keys(createProjectMachine.states)).not.toContain("pathing");
    expect(Object.keys(STATE_TO_SLUG)).not.toContain("pathing");
  });
});
