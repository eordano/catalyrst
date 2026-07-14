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
  simulateScaffold,
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
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(createProjectMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.naming);
    expect(slugToState(null)).toBe("naming");
    expect(slugToState(undefined)).toBe("naming");
    expect(slugToState("")).toBe("naming");
    expect(slugToState("nope")).toBe("naming");
    expect(slugToState("path")).toBe("naming");
    expect(slugToState("template")).toBe("templating");
    expect(slugToState("scaffold")).toBe("scaffolding");
    expect(slugToState("created")).toBe("created");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("createProjectMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveCreateProjectSnapshot({
      step: "naming",
      trackCtx: inputFor(okScaffold, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("run-scoped steps (scaffold/created/error) are NOT cold-resumable \u{2014} they fall back to naming", async () => {
    const track = vi.fn();
    const scaffold = vi.fn(okScaffold);

    for (const step of ["scaffolding", "created", "error"] as const) {
      expect(COLD_RESUMABLE_STATES).not.toContain(step);
      const snapshot = resolveCreateProjectSnapshot({
        step,
        trackCtx: inputFor(scaffold, track).trackCtx,
        scaffold,
        track,
        name: "My Awesome Scene",
        template: "art-gallery",
      });
      expect(snapshot).toBeUndefined();
    }

    const actor = createActor(createProjectMachine, {
      input: inputFor(scaffold, track),
      snapshot: undefined,
    }).start();
    expect(actor.getSnapshot().matches("naming")).toBe(true);
    expect(actor.getSnapshot().context.result).toBeUndefined();

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(scaffold).not.toHaveBeenCalled();
  });

  it("hydrating templating does NOT fire telemetry, does NOT auto-scaffold, and seeds slug + path from the name", async () => {
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
    await waitFor(actor, (s) => s.matches("created"));
    expect(actor.getSnapshot().context.projectSlug).toBe("neon-market");
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveCreateProjectSnapshot({
      step: "templating",
      trackCtx: inputFor(okScaffold, track).trackCtx,
      track,
    });
    const actor = createActor(createProjectMachine, {
      input: inputFor(okScaffold, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("templating")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SELECT_TEMPLATE", template: "empty" });
    expect(track.mock.calls.map((c) => c[0])).toContain(
      CREATE_PROJECT_EVENTS.templateSelected,
    );
  });
});

describe("createProjectMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
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
    expect(ends.has("naming")).toBe(true);
    expect(ends.has("templating")).toBe(true);
    expect(ends.has("scaffolding")).toBe(true);
  });

  it("reaching scaffolding passes through a valid name and a template", () => {
    const paths = getShortestPaths(createProjectMachine, {
      input: inputFor(okScaffold, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const scaffolding = paths.find((p) => (p.state.value as string) === "scaffolding");
    expect(scaffolding).toBeDefined();
    const events = scaffolding!.steps.map((s) => s.event.type);
    expect(events).toContain("SET_NAME");
    expect(events).toContain("SELECT_TEMPLATE");
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
    expect(events).toContain(CREATE_PROJECT_EVENTS.started);
    expect(events).toContain(CREATE_PROJECT_EVENTS.nameSet);
    expect(events).toContain(CREATE_PROJECT_EVENTS.pathSet);
    expect(events).toContain(CREATE_PROJECT_EVENTS.templateSelected);
    expect(events).toContain(CREATE_PROJECT_EVENTS.scaffolding);
    expect(events).toContain(CREATE_PROJECT_EVENTS.completed);

    expect(events.indexOf(CREATE_PROJECT_EVENTS.started)).toBeLessThan(
      events.indexOf(CREATE_PROJECT_EVENTS.completed),
    );

    expect(events).not.toContain(CREATE_PROJECT_EVENTS.pathInvalid);

    const startedCall = track.mock.calls.find(
      (c) => c[0] === CREATE_PROJECT_EVENTS.started,
    );
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "ch_create_project_wizard",
      variant: "wizard",
    });

    const completedCall = track.mock.calls.find(
      (c) => c[0] === CREATE_PROJECT_EVENTS.completed,
    );
    expect(completedCall?.[1]).toMatchObject({ written: false, template: "art-gallery" });
    expect(completedCall?.[1].files).toEqual(SCAFFOLD_FILES.map((f) => f.name));
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("started fires exactly once even across BACK/forward in the name step", () => {
    const track = vi.fn();
    const actor = createActor(createProjectMachine, {
      input: inputFor(okScaffold, track),
    }).start();

    actor.send({ type: "SET_NAME", name: "A" });
    actor.send({ type: "BACK" });
    actor.send({ type: "SET_NAME", name: "B" });

    const startedCount = track.mock.calls.filter(
      (c) => c[0] === CREATE_PROJECT_EVENTS.started,
    ).length;
    expect(startedCount).toBe(2);
  });
});

describe("createProjectMachine \u{2014} invalid name guardrail", () => {
  it("an invalid SET_NAME stays in naming and fires ch_create_project_path_invalid", () => {
    const track = vi.fn();
    const actor = createActor(createProjectMachine, {
      input: inputFor(okScaffold, track),
    }).start();

    actor.send({ type: "SET_NAME", name: "Taken", valid: false });

    expect(actor.getSnapshot().matches("naming")).toBe(true);
    expect(actor.getSnapshot().context.name).toBe("Taken");

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CREATE_PROJECT_EVENTS.pathInvalid);
    expect(events).not.toContain(CREATE_PROJECT_EVENTS.pathSet);

    actor.send({ type: "SET_NAME", name: "Free", valid: true });
    expect(actor.getSnapshot().matches("templating")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(CREATE_PROJECT_EVENTS.pathSet);
  });
});

describe("createProjectMachine \u{2014} preselected template skips templating", () => {
  it("a valid SET_NAME goes straight to scaffolding when template came from the URL", async () => {
    const track = vi.fn();
    const actor = createActor(createProjectMachine, {
      input: { ...inputFor(okScaffold, track), template: "tower-defense" },
    }).start();

    actor.send({ type: "SET_NAME", name: "Tower Defense", valid: true });
    expect(actor.getSnapshot().matches("templating")).toBe(false);
    await waitFor(actor, (s) => s.matches("created"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CREATE_PROJECT_EVENTS.templateSelected);
    const selectedCall = track.mock.calls.find(
      (c) => c[0] === CREATE_PROJECT_EVENTS.templateSelected,
    );
    expect(selectedCall?.[1]).toMatchObject({
      template: "tower-defense",
      preselected: true,
    });
    expect(actor.getSnapshot().context.template).toBe("tower-defense");
  });

  it("an invalid SET_NAME still stays in naming with a preselected template", () => {
    const actor = createActor(createProjectMachine, {
      input: { ...inputFor(okScaffold, vi.fn()), template: "memory-game" },
    }).start();

    actor.send({ type: "SET_NAME", name: "Taken", valid: false });
    expect(actor.getSnapshot().matches("naming")).toBe(true);
  });

  it("without a preselected template a valid SET_NAME still lands on templating", () => {
    const actor = createActor(createProjectMachine, {
      input: inputFor(okScaffold, vi.fn()),
    }).start();

    actor.send({ type: "SET_NAME", name: "My Scene", valid: true });
    expect(actor.getSnapshot().matches("templating")).toBe(true);
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

describe("createProjectMachine \u{2014} name error surface + project slug", () => {
  it("invalid SET_NAME records the given error and stays in naming (no slug yet)", () => {
    const actor = createActor(createProjectMachine, {
      input: inputFor(okScaffold, () => {}),
    }).start();

    actor.send({
      type: "SET_NAME",
      name: "Taken",
      valid: false,
      error: "Name already in use",
    });

    const snap = actor.getSnapshot();
    expect(snap.matches("naming")).toBe(true);
    expect(snap.context.pathError).toBe("Name already in use");
    expect(snap.context.projectSlug).toBeUndefined();
  });

  it("invalid SET_NAME without a message still surfaces a default reason", () => {
    const actor = createActor(createProjectMachine, {
      input: inputFor(okScaffold, () => {}),
    }).start();

    actor.send({ type: "SET_NAME", name: "X", valid: false });

    expect(actor.getSnapshot().context.pathError).toBeTruthy();
  });

  it("a valid name clears the error, sets the slug, and carries it to created", async () => {
    const actor = createActor(createProjectMachine, {
      input: inputFor(okScaffold, () => {}),
    }).start();

    actor.send({ type: "SET_NAME", name: "Taken", valid: false, error: "nope" });
    expect(actor.getSnapshot().context.pathError).toBe("nope");
    expect(actor.getSnapshot().matches("naming")).toBe(true);

    actor.send({ type: "SET_NAME", name: "My Awesome Scene", valid: true });
    const mid = actor.getSnapshot();
    expect(mid.matches("templating")).toBe(true);
    expect(mid.context.pathError).toBeUndefined();
    expect(mid.context.projectSlug).toBe("my-awesome-scene");
    expect(mid.context.path).toBe("my-awesome-scene");

    actor.send({ type: "SELECT_TEMPLATE", template: "empty" });
    await waitFor(actor, (s) => s.matches("created"));
    expect(actor.getSnapshot().context.projectSlug).toBe("my-awesome-scene");
  });
});

describe("slugifyProjectName", () => {
  it("derives a url-safe slug and falls back for empty/symbol-only names", () => {
    expect(slugifyProjectName("My Awesome Scene")).toBe("my-awesome-scene");
    expect(slugifyProjectName("  Neon   Market!! ")).toBe("neon-market");
    expect(slugifyProjectName("***")).toBe("new-scene");
    expect(slugifyProjectName("")).toBe("new-scene");
  });
});

describe("CreateProjectView \u{2014} created screen create->edit seam", () => {
  const result: ScaffoldResult = {
    files: SCAFFOLD_FILES,
    via: "directory",
    folder: "my-awesome-scene",
    written: true,
  };

  it("offers a single primary 'Open in editor' deep link + secondary 'Go to My Scenes'", () => {
    const html = renderToStaticMarkup(
      createElement(CreateProjectView, {
        view: "created",
        name: "My Awesome Scene",
        projectSlug: "my-awesome-scene",
        signedIn: true,
        result,
      }),
    );

    expect(html).toContain("Open in editor");
    expect(html).toContain("/creator-hub/scene-editor?source=local");
    expect(html).toContain("project=my-awesome-scene");
    expect(html).toContain('href="/create/scenes"');
    expect(html).toContain("Go to My Scenes");
    expect((html.match(/create-project-wizard__btn--primary/g) ?? []).length).toBe(1);
  });

  it("shows a not-signed-in note (publishing needs a wallet) but no gate", () => {
    const html = renderToStaticMarkup(
      createElement(CreateProjectView, {
        view: "created",
        name: "Scene",
        projectSlug: "scene",
        signedIn: false,
        result,
      }),
    );
    expect(html).toMatch(/publishing this scene later needs a connected wallet/i);
    expect(html).toContain("Open in editor");
  });

  it("never fabricates file-write claims when there is no scaffold result", () => {
    const html = renderToStaticMarkup(
      createElement(CreateProjectView, {
        view: "created",
        name: "Scene",
        projectSlug: "scene",
        signedIn: true,
      }),
    );
    expect(html).not.toContain("0 files");
    expect(html).not.toMatch(/Wrote/);
    expect(html).toMatch(/Your scene is ready/);
    expect(html).toContain("Open in editor");
  });

  it("surfaces the name error inline in the naming modal (role=alert)", () => {
    const html = renderToStaticMarkup(
      createElement(CreateProjectView, {
        view: "naming",
        pathError: "A scene with that name already exists.",
      }),
    );
    expect(html).toContain("A scene with that name already exists.");
    expect(html).toContain('role="alert"');
  });
});

describe("CreateProjectView \u{2014} dismissed-picker error offers folder recovery", () => {
  it("a cancellation error shows [Choose folder again] as the primary action", () => {
    const html = renderToStaticMarkup(
      createElement(CreateProjectView, {
        view: "error",
        error: "Scene creation canceled \u{2014} no folder was chosen.",
      }),
    );
    expect(html).toContain(
      "Scene creation was cancelled \u{2014} no folder was chosen.",
    );
    expect(html).toContain("Choose folder again");
    expect(html).toContain("Retry");
    expect((html.match(/btn btn--primary/g) ?? []).length).toBe(1);
  });
});

describe("simulateScaffold", () => {
  it("resolves the canonical SDK7 file list (no disk write)", async () => {
    const out = await simulateScaffold({
      name: "x",
      path: "/x",
      template: "empty",
    });
    expect(out.files.map((f) => f.name)).toEqual([
      "scene.json",
      "package.json",
      "tsconfig.json",
      "src/index.ts",
      ".gitignore",
    ]);
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
