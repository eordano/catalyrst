import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type ScaffoldFile = { name: string; note: string };

export const DEFAULT_TEMPLATE = "empty";

export type { TrackFn };

export type ScaffoldResult = {
  files: ScaffoldFile[];
  written?: boolean;
  via?: "directory" | "download" | "canceled";
  folder?: string;
  dir?: FileSystemDirectoryHandle | null;
};

export type ScaffoldFn = (args: {
  name: string;
  path: string;
  template: string;
  signal?: AbortSignal;
}) => Promise<ScaffoldResult>;

export type CreateProjectInput = {
  trackCtx: TrackContext;
  scaffold?: ScaffoldFn;
  track?: TrackFn;
  template?: string;
  name?: string;
  path?: string;
};

export type CreateProjectContext = {
  trackCtx: TrackContext;
  scaffold: ScaffoldFn;
  track: TrackFn;
  name: string;
  path: string;
  template: string;
  templatePreselected: boolean;
  projectSlug?: string;
  pathError?: string;
  result?: ScaffoldResult;
  error?: string;
};

export type CreateProjectEvent =
  | { type: "SET_NAME"; name: string; valid?: boolean; error?: string }
  | { type: "SELECT_TEMPLATE"; template: string }
  | { type: "BACK" }
  | { type: "RETRY" }
  | { type: "CHOOSE_FOLDER" };

export function slugifyProjectName(name: string): string {
  return (
    name
      .toLowerCase()
      .trim()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "new-scene"
  );
}

export const CREATE_PROJECT_EVENTS = {
  started: "ch_create_project_started",
  nameSet: "ch_create_project_name_set",
  pathSet: "ch_create_project_path_set",
  pathInvalid: "ch_create_project_path_invalid",
  templateSelected: "ch_create_project_template_selected",
  scaffolding: "ch_create_project_scaffolding",
  completed: "ch_create_project_completed",
  failed: "ch_create_project_failed",
} as const;

export const STATE_TO_SLUG = {
  naming: "name",
  templating: "template",
  scaffolding: "scaffold",
  created: "created",
  error: "error",
} as const;

export type CreateProjectStateId = keyof typeof STATE_TO_SLUG;
export type CreateProjectStepSlug = (typeof STATE_TO_SLUG)[CreateProjectStateId];

export const FIRST_STEP_SLUG: CreateProjectStepSlug = STATE_TO_SLUG.naming;

export const COLD_RESUMABLE_STATES: readonly CreateProjectStateId[] = [
  "naming",
  "templating",
];

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "naming");

export const SLUG_TO_STATE: Record<CreateProjectStepSlug, CreateProjectStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => CreateProjectStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => CreateProjectStateId = stepSlugs.toState;

export const SCAFFOLD_FILES: ScaffoldFile[] = [
  { name: "scene.json", note: "scene metadata, parcels, spawn points" },
  { name: "package.json", note: "@dcl/sdk dependency + scripts" },
  { name: "tsconfig.json", note: "SDK7 TypeScript config" },
  { name: "src/index.ts", note: "main() entry point" },
  { name: ".gitignore", note: "node_modules, bin, .DS_Store" },
];

export const simulateScaffold: ScaffoldFn = async ({ signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return { files: SCAFFOLD_FILES };
};

export const createProjectMachine = setup({
  types: {
    context: {} as CreateProjectContext,
    events: {} as CreateProjectEvent,
    input: {} as CreateProjectInput,
  },
  actors: {
    runScaffold: fromPromise<
      ScaffoldResult,
      { name: string; path: string; template: string; scaffold: ScaffoldFn }
    >(({ input, signal }) =>
      input.scaffold({
        name: input.name,
        path: input.path,
        template: input.template,
        signal,
      }),
    ),
  },
  guards: {
    isNameValid: ({ event }) => event.type === "SET_NAME" && event.valid !== false,
    isNameValidWithTemplate: ({ context, event }) =>
      event.type === "SET_NAME" &&
      event.valid !== false &&
      context.templatePreselected,
  },
  actions: {
    setName: assign({
      name: ({ context, event }) =>
        event.type === "SET_NAME" ? event.name : context.name,
    }),
    trackStarted: ({ context }) =>
      context.track(CREATE_PROJECT_EVENTS.started, {}, context.trackCtx),
    trackNameSet: ({ context, event }) => {
      if (event.type !== "SET_NAME") return;
      context.track(
        CREATE_PROJECT_EVENTS.nameSet,
        { name: event.name },
        context.trackCtx,
      );
    },
    setDerivedPath: assign({
      path: ({ context }) => slugifyProjectName(context.name),
    }),
    clearPathError: assign({ pathError: undefined }),
    setPathError: assign({
      pathError: ({ context, event }) => {
        if (event.type !== "SET_NAME") return context.pathError;
        return event.error ?? "That name is already taken \u{2014} choose a different one.";
      },
    }),
    setProjectSlug: assign({
      projectSlug: ({ context }) => slugifyProjectName(context.name),
    }),
    trackPathSet: ({ context }) =>
      context.track(
        CREATE_PROJECT_EVENTS.pathSet,
        { path: context.path },
        context.trackCtx,
      ),
    trackPathInvalid: ({ context, event }) => {
      if (event.type !== "SET_NAME") return;
      context.track(
        CREATE_PROJECT_EVENTS.pathInvalid,
        { path: slugifyProjectName(event.name) },
        context.trackCtx,
      );
    },
    setTemplate: assign({
      template: ({ context, event }) =>
        event.type === "SELECT_TEMPLATE" ? event.template : context.template,
    }),
    trackTemplateSelected: ({ context, event }) => {
      if (event.type !== "SELECT_TEMPLATE") return;
      context.track(
        CREATE_PROJECT_EVENTS.templateSelected,
        { template: event.template },
        context.trackCtx,
      );
    },
    trackPreselectedTemplate: ({ context }) =>
      context.track(
        CREATE_PROJECT_EVENTS.templateSelected,
        { template: context.template, preselected: true },
        context.trackCtx,
      ),
    trackScaffolding: ({ context }) =>
      context.track(
        CREATE_PROJECT_EVENTS.scaffolding,
        { name: context.name, path: context.path, template: context.template },
        context.trackCtx,
      ),
    trackCompleted: ({ context }) =>
      context.track(
        CREATE_PROJECT_EVENTS.completed,
        {
          name: context.name,
          path: context.path,
          template: context.template,
          files: context.result?.files.map((f) => f.name) ?? [],
          written: context.result?.written ?? false,
          via: context.result?.via,
          folder: context.result?.folder,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "createProjectWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    scaffold: input.scaffold ?? simulateScaffold,
    track: input.track ?? defaultTrack,
    name: input.name ?? "My Awesome Scene",
    path: input.path ?? "",
    template: input.template ?? DEFAULT_TEMPLATE,
    templatePreselected: input.template != null,
  }),
  initial: "naming",
  states: {
    naming: {
      on: {
        SET_NAME: [
          {
            guard: "isNameValidWithTemplate",
            target: "scaffolding",
            actions: [
              "trackStarted",
              "setName",
              "setDerivedPath",
              "setProjectSlug",
              "clearPathError",
              "trackNameSet",
              "trackPathSet",
              "trackPreselectedTemplate",
            ],
          },
          {
            guard: "isNameValid",
            target: "templating",
            actions: [
              "trackStarted",
              "setName",
              "setDerivedPath",
              "setProjectSlug",
              "clearPathError",
              "trackNameSet",
              "trackPathSet",
            ],
          },
          {
            actions: [
              "trackStarted",
              "setName",
              "setPathError",
              "trackNameSet",
              "trackPathInvalid",
            ],
          },
        ],
      },
    },
    templating: {
      on: {
        SELECT_TEMPLATE: {
          target: "scaffolding",
          actions: ["setTemplate", "trackTemplateSelected"],
        },
        BACK: { target: "naming" },
      },
    },
    scaffolding: {
      entry: [assign({ error: undefined }), "trackScaffolding"],
      invoke: {
        id: "runScaffold",
        src: "runScaffold",
        input: ({ context }) => ({
          name: context.name,
          path: context.path,
          template: context.template,
          scaffold: context.scaffold,
        }),
        onDone: {
          target: "created",
          actions: [assign({ result: ({ event }) => event.output }), "trackCompleted"],
        },
        onError: {
          target: "error",
          actions: [
            ({ context, event }) => {
              const raw: unknown = event.error;
              const code =
                raw && typeof raw === "object" && "code" in raw &&
                (raw as { code?: unknown }).code
                  ? String((raw as { code?: unknown }).code)
                  : raw instanceof Error
                    ? raw.name
                    : undefined;
              const message =
                toErrorMessage(raw, "scaffold failed");
              context.track(
                CREATE_PROJECT_EVENTS.failed,
                { code, message },
                context.trackCtx,
              );
            },
            assign({
              error: ({ event }) => {
                const raw = event.error;
                if (raw instanceof Error) {
                  return raw.name && raw.name !== "Error"
                    ? `${raw.name}: ${raw.message}`
                    : raw.message;
                }
                return String(raw ?? "scaffold failed");
              },
            }),
          ],
        },
      },
    },
    created: {
      type: "final",
    },
    error: {
      on: {
        RETRY: { target: "scaffolding" },
        CHOOSE_FOLDER: {
          target: "scaffolding",
          actions: assign({ error: undefined }),
        },
      },
    },
  },
});

export type CreateProjectMachine = typeof createProjectMachine;

export function resolveCreateProjectSnapshot(args: {
  step: CreateProjectStateId;
  trackCtx: TrackContext;
  scaffold?: ScaffoldFn;
  track?: TrackFn;
  name?: string;
  path?: string;
  template?: string;
}) {
  const { step, trackCtx, scaffold, track, name, path, template } = args;
  if (step === "naming" || !COLD_RESUMABLE_STATES.includes(step)) return undefined;
  const resumedName = name ?? "My Awesome Scene";
  const context: CreateProjectContext = {
    trackCtx,
    scaffold: scaffold ?? simulateScaffold,
    track: track ?? defaultTrack,
    name: resumedName,
    path: path || slugifyProjectName(resumedName),
    template: template ?? DEFAULT_TEMPLATE,
    templatePreselected: template != null,
    projectSlug: slugifyProjectName(resumedName),
  };
  return createProjectMachine.resolveState({ value: step, context });
}
