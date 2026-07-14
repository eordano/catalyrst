import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type PublishResult = {
  updateId: string;
};

export type PublishFn = (args: {
  projectId: string;
  health: string;
  signal?: AbortSignal;
}) => Promise<PublishResult>;

export type { TrackFn };

export type UpdateDraft = {
  health: string;
  introduction: string;
  highlights: string;
  blockers: string;
  next_steps: string;
  additional_notes: string;
  csv: string;
  disclosed: number;
  records: number;
};

export type UpdateInput = {
  trackCtx: TrackContext;
  projectId: string;
  publishUpdate?: PublishFn;
  track?: TrackFn;
  draft?: Partial<UpdateDraft>;
};

export type UpdateContext = {
  trackCtx: TrackContext;
  projectId: string;
  publishUpdate: PublishFn;
  track: TrackFn;
  draft: UpdateDraft;
  result?: PublishResult;
  error?: string;
};

export type UpdateEvent =
  | { type: "SET_GENERAL"; health: string; fields?: Partial<UpdateDraft> }
  | { type: "SET_FINANCIALS"; csv: string; disclosed: number; records: number }
  | { type: "NEXT" }
  | { type: "BACK" }
  | { type: "PUBLISH" }
  | { type: "RETRY" };

export const UPDATE_EVENTS = {
  started: "gv_update_started",
  financialsSet: "gv_update_financials_set",
  previewed: "gv_update_previewed",
  publishAttempted: "gv_update_publish_attempted",
  published: "gv_update_published",
} as const;

export const STATE_TO_SLUG = {
  general: "general",
  financials: "financials",
  preview: "preview",
  publishing: "publishing",
  publishError: "publish-error",
  success: "success",
} as const;

export type UpdateStateId = keyof typeof STATE_TO_SLUG;
export type UpdateStepSlug = (typeof STATE_TO_SLUG)[UpdateStateId];

export const FIRST_STEP_SLUG: UpdateStepSlug = STATE_TO_SLUG.general;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "general");

export const SLUG_TO_STATE: Record<UpdateStepSlug, UpdateStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => UpdateStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => UpdateStateId = stepSlugs.toState;

export const simulatePublish: PublishFn = async ({ projectId, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const slug = projectId.slice(0, 8) || "project";
  return { updateId: `stub-update-${slug}-${Date.now().toString(36)}` };
};

const DEFAULT_DRAFT: UpdateDraft = {
  health: "onTrack",
  introduction: "",
  highlights: "",
  blockers: "",
  next_steps: "",
  additional_notes: "",
  csv: "category,description,token,amount,receiver,link",
  disclosed: 0,
  records: 0,
};

export const projectUpdateMachine = setup({
  types: {
    context: {} as UpdateContext,
    events: {} as UpdateEvent,
    input: {} as UpdateInput,
  },
  actors: {
    runPublish: fromPromise<
      PublishResult,
      { projectId: string; health: string; publishUpdate: PublishFn }
    >(({ input, signal }) =>
      input.publishUpdate({
        projectId: input.projectId,
        health: input.health,
        signal,
      }),
    ),
  },
  actions: {
    setGeneral: assign({
      draft: ({ context, event }) =>
        event.type === "SET_GENERAL"
          ? { ...context.draft, health: event.health, ...(event.fields ?? {}) }
          : context.draft,
    }),
    setFinancials: assign({
      draft: ({ context, event }) =>
        event.type === "SET_FINANCIALS"
          ? {
              ...context.draft,
              csv: event.csv,
              disclosed: event.disclosed,
              records: event.records,
            }
          : context.draft,
    }),
    trackStarted: ({ context }) =>
      context.track(
        UPDATE_EVENTS.started,
        { health: context.draft.health, project_id: context.projectId },
        context.trackCtx,
      ),
    trackFinancialsSet: ({ context }) =>
      context.track(
        UPDATE_EVENTS.financialsSet,
        {
          disclosed: context.draft.disclosed,
          records: context.draft.records,
          project_id: context.projectId,
        },
        context.trackCtx,
      ),
    trackPreviewed: ({ context }) =>
      context.track(
        UPDATE_EVENTS.previewed,
        { health: context.draft.health, project_id: context.projectId },
        context.trackCtx,
      ),
    trackPublishAttempted: ({ context }) =>
      context.track(
        UPDATE_EVENTS.publishAttempted,
        {
          health: context.draft.health,
          disclosed: context.draft.disclosed,
          project_id: context.projectId,
        },
        context.trackCtx,
      ),
    trackPublished: ({ context }) =>
      context.track(
        UPDATE_EVENTS.published,
        {
          update_id: context.result?.updateId,
          project_id: context.projectId,
          health: context.draft.health,
          simulated: true,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "projectUpdateSubmit",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    projectId: input.projectId,
    publishUpdate: input.publishUpdate ?? simulatePublish,
    track: input.track ?? defaultTrack,
    draft: { ...DEFAULT_DRAFT, ...(input.draft ?? {}) },
  }),
  initial: "general",
  states: {
    general: {
      on: {
        SET_GENERAL: { actions: "setGeneral" },
        NEXT: { target: "financials", actions: "trackStarted" },
      },
    },
    financials: {
      on: {
        SET_FINANCIALS: { actions: "setFinancials" },
        NEXT: {
          target: "preview",
          actions: ["setFinancials", "trackFinancialsSet"],
        },
        BACK: { target: "general" },
      },
    },
    preview: {
      entry: "trackPreviewed",
      on: {
        PUBLISH: { target: "publishing", actions: "trackPublishAttempted" },
        BACK: { target: "financials" },
      },
    },
    publishing: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runPublish",
        src: "runPublish",
        input: ({ context }) => ({
          projectId: context.projectId,
          health: context.draft.health,
          publishUpdate: context.publishUpdate,
        }),
        onDone: {
          target: "success",
          actions: [
            assign({ result: ({ event }) => event.output }),
            "trackPublished",
          ],
        },
        onError: {
          target: "publishError",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "publish failed"),
          }),
        },
      },
    },
    publishError: {
      on: {
        RETRY: { target: "publishing" },
        BACK: { target: "preview" },
      },
    },
    success: {
      type: "final",
    },
  },
});

export type ProjectUpdateMachine = typeof projectUpdateMachine;

export function resolveUpdateSnapshot(args: {
  step: UpdateStateId;
  trackCtx: TrackContext;
  projectId: string;
  publishUpdate?: PublishFn;
  track?: TrackFn;
  draft?: Partial<UpdateDraft>;
}) {
  const { step, trackCtx, projectId, publishUpdate, track, draft } = args;
  if (step === "general") return undefined;
  const seeded: UpdateDraft = {
    ...DEFAULT_DRAFT,
    health: "onTrack",
    introduction:
      "This update covers the last reporting period for the project.",
    highlights: "Shipped the planned milestones and improved performance.",
    blockers: "No blockers this period.",
    next_steps: "Continue with the roadmap and start the next milestone.",
    ...(draft ?? {}),
  };
  const context: UpdateContext = {
    trackCtx,
    projectId,
    publishUpdate: publishUpdate ?? simulatePublish,
    track: track ?? defaultTrack,
    draft: seeded,
  };
  return projectUpdateMachine.resolveState({ value: step, context });
}
