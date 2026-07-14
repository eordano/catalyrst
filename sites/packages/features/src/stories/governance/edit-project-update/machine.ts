import { assign, fromPromise, setup } from "xstate";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";
import type { ProjectHealth } from "@data/lib/catalyst/governance/edit-project-update";

export type SaveResult = {
  updateId: string;
};

export type SaveFn = (args: {
  projectId: string;
  updateId: string;
  signal?: AbortSignal;
}) => Promise<SaveResult>;

export type { TrackFn };

export type EditDraft = {
  projectId: string;
  updateId: string;
  health: ProjectHealth;
  introduction: string;
  highlights: string;
  blockers: string;
  next_steps: string;
  additional_notes: string;
  recordCount: number;
};

export type EditInput = {
  trackCtx: TrackContext;
  draft: EditDraft;
  saveUpdate?: SaveFn;
  track?: TrackFn;
};

export type EditContext = {
  trackCtx: TrackContext;
  draft: EditDraft;
  saveUpdate: SaveFn;
  track: TrackFn;
  result?: SaveResult;
  error?: string;
};

export type EditEvent =
  | { type: "NEXT" }
  | { type: "REVIEW" }
  | { type: "SAVE" }
  | { type: "CANCEL" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const EDIT_EVENTS = {
  started: "gv_update_edit_started",
  financials: "gv_update_edit_financials",
  confirmOpen: "gv_update_edit_confirm_open",
  saveAttempted: "gv_update_edit_save_attempted",
  saved: "gv_update_edit_saved",
} as const;

export const STATE_TO_SLUG = {
  general: "general",
  financials: "financials",
  confirm: "confirm",
  saving: "saving",
  saveError: "save-error",
  done: "done",
} as const;

export type EditStateId = keyof typeof STATE_TO_SLUG;
export type EditStepSlug = (typeof STATE_TO_SLUG)[EditStateId];

export const FIRST_STEP_SLUG: EditStepSlug = STATE_TO_SLUG.general;

export const SLUG_TO_STATE = Object.fromEntries(
  Object.entries(STATE_TO_SLUG).map(([state, slug]) => [slug, state]),
) as Record<EditStepSlug, EditStateId>;

export function stateToSlug(value: string): EditStepSlug {
  return STATE_TO_SLUG[value as EditStateId] ?? FIRST_STEP_SLUG;
}

export function slugToState(slug: string | null | undefined): EditStateId {
  if (!slug) return "general";
  return SLUG_TO_STATE[slug as EditStepSlug] ?? "general";
}

export const simulateSave: SaveFn = async ({ updateId, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return { updateId };
};

export const editUpdateMachine = setup({
  types: {
    context: {} as EditContext,
    events: {} as EditEvent,
    input: {} as EditInput,
  },
  actors: {
    runSave: fromPromise<
      SaveResult,
      { projectId: string; updateId: string; saveUpdate: SaveFn }
    >(({ input, signal }) =>
      input.saveUpdate({
        projectId: input.projectId,
        updateId: input.updateId,
        signal,
      }),
    ),
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(
        EDIT_EVENTS.started,
        {
          project_id: context.draft.projectId,
          update_id: context.draft.updateId,
          health: context.draft.health,
        },
        context.trackCtx,
      ),
    trackFinancials: ({ context }) =>
      context.track(
        EDIT_EVENTS.financials,
        {
          project_id: context.draft.projectId,
          update_id: context.draft.updateId,
          records: context.draft.recordCount,
        },
        context.trackCtx,
      ),
    trackConfirmOpen: ({ context }) =>
      context.track(
        EDIT_EVENTS.confirmOpen,
        {
          project_id: context.draft.projectId,
          update_id: context.draft.updateId,
        },
        context.trackCtx,
      ),
    trackSaveAttempted: ({ context }) =>
      context.track(
        EDIT_EVENTS.saveAttempted,
        {
          project_id: context.draft.projectId,
          update_id: context.draft.updateId,
        },
        context.trackCtx,
      ),
    trackSaved: ({ context }) =>
      context.track(
        EDIT_EVENTS.saved,
        {
          project_id: context.draft.projectId,
          update_id: context.result?.updateId ?? context.draft.updateId,
          simulated: true,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "editProjectUpdate",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    draft: input.draft,
    saveUpdate: input.saveUpdate ?? simulateSave,
    track: input.track ?? defaultTrack,
  }),
  initial: "general",
  states: {
    general: {
      on: {
        NEXT: { target: "financials", actions: "trackStarted" },
      },
    },
    financials: {
      on: {
        REVIEW: { target: "confirm", actions: "trackFinancials" },
        BACK: { target: "general" },
      },
    },
    confirm: {
      entry: "trackConfirmOpen",
      on: {
        SAVE: { target: "saving", actions: "trackSaveAttempted" },
        CANCEL: { target: "financials" },
      },
    },
    saving: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSave",
        src: "runSave",
        input: ({ context }) => ({
          projectId: context.draft.projectId,
          updateId: context.draft.updateId,
          saveUpdate: context.saveUpdate,
        }),
        onDone: {
          target: "done",
          actions: [
            assign({ result: ({ event }) => event.output }),
            "trackSaved",
          ],
        },
        onError: {
          target: "saveError",
          actions: assign({
            error: ({ event }) =>
              event.error instanceof Error
                ? event.error.message
                : String(event.error ?? "save failed"),
          }),
        },
      },
    },
    saveError: {
      on: {
        RETRY: { target: "saving" },
        CANCEL: { target: "confirm" },
      },
    },
    done: {
      type: "final",
    },
  },
});

export type EditUpdateMachine = typeof editUpdateMachine;

export function resolveEditSnapshot(args: {
  step: EditStateId;
  trackCtx: TrackContext;
  draft: EditDraft;
  saveUpdate?: SaveFn;
  track?: TrackFn;
}) {
  const { step, trackCtx, draft, saveUpdate, track } = args;
  if (step === "general") return undefined;
  const context: EditContext = {
    trackCtx,
    draft,
    saveUpdate: saveUpdate ?? simulateSave,
    track: track ?? defaultTrack,
  };
  return editUpdateMachine.resolveState({ value: step, context });
}
