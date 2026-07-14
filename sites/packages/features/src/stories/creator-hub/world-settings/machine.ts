import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type SettingsTab = "details" | "layout" | "misc";

export type Ui3Tab = "details" | "layout" | "general";

export function tabToUi3(tab: SettingsTab): Ui3Tab {
  return tab === "misc" ? "general" : tab;
}

export type { TrackFn };

export type SaveResult = {
  worldName: string;
  savedFields: string[];
  stub?: boolean;
};

export type SaveFn = (args: {
  worldName: string;
  changes: Record<string, true>;
  signal?: AbortSignal;
}) => Promise<SaveResult>;

export type WorldSettingsInput = {
  trackCtx: TrackContext;
  worldName?: string;
  save?: SaveFn;
  track?: TrackFn;
};

export type WorldSettingsCtx = {
  trackCtx: TrackContext;
  worldName: string;
  save: SaveFn;
  track: TrackFn;
  changes: Record<string, true>;
  result?: SaveResult;
  error?: string;
};

export type WorldSettingsEvent =
  | { type: "NEXT" }
  | { type: "BACK" }
  | { type: "GO_TAB"; tab: SettingsTab }
  | { type: "CHANGE"; tab: SettingsTab; field: string }
  | { type: "REVIEW" }
  | { type: "DISCARD" }
  | { type: "SAVE" }
  | { type: "RETRY" };

export const WORLD_SETTINGS_EVENTS = {
  opened: "ch_world_settings_opened",
  tabViewed: "ch_world_settings_tab_viewed",
  changed: "ch_world_settings_changed",
  reviewReached: "ch_world_settings_review_reached",
  discarded: "ch_world_settings_discarded",
  saving: "ch_world_settings_saving",
  saved: "ch_world_settings_saved",
} as const;

export const STATE_TO_SLUG = {
  details: "details",
  layout: "layout",
  misc: "misc",
  review: "review",
  saving: "saving",
  saved: "saved",
  error: "error",
} as const;

export type WorldSettingsStateId = keyof typeof STATE_TO_SLUG;
export type WorldSettingsStepSlug = (typeof STATE_TO_SLUG)[WorldSettingsStateId];

export const FIRST_STEP_SLUG: WorldSettingsStepSlug = STATE_TO_SLUG.details;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "details");

export const SLUG_TO_STATE: Record<WorldSettingsStepSlug, WorldSettingsStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => WorldSettingsStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => WorldSettingsStateId = stepSlugs.toState;

export const simulateSave: SaveFn = async ({ worldName, changes, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return { worldName, savedFields: Object.keys(changes), stub: true };
};

export const worldSettingsMachine = setup({
  types: {
    context: {} as WorldSettingsCtx,
    events: {} as WorldSettingsEvent,
    input: {} as WorldSettingsInput,
  },
  actors: {
    runSave: fromPromise<
      SaveResult,
      { worldName: string; changes: Record<string, true>; save: SaveFn }
    >(({ input, signal }) =>
      input.save({ worldName: input.worldName, changes: input.changes, signal }),
    ),
  },
  actions: {
    trackOpened: ({ context }) =>
      context.track(
        WORLD_SETTINGS_EVENTS.opened,
        { world: context.worldName },
        context.trackCtx,
      ),
    trackTabViewed: ({ context }, params: { tab: SettingsTab }) =>
      context.track(
        WORLD_SETTINGS_EVENTS.tabViewed,
        { tab: params.tab, world: context.worldName },
        context.trackCtx,
      ),
    recordChange: assign({
      changes: ({ context, event }) => {
        if (event.type !== "CHANGE") return context.changes;
        return { ...context.changes, [`${event.tab}.${event.field}`]: true as const };
      },
    }),
    trackChanged: ({ context, event }) => {
      if (event.type !== "CHANGE") return;
      context.track(
        WORLD_SETTINGS_EVENTS.changed,
        { tab: event.tab, field: event.field, world: context.worldName },
        context.trackCtx,
      );
    },
    trackReviewReached: ({ context }) =>
      context.track(
        WORLD_SETTINGS_EVENTS.reviewReached,
        { world: context.worldName, change_count: Object.keys(context.changes).length },
        context.trackCtx,
      ),
    trackDiscarded: ({ context }) =>
      context.track(
        WORLD_SETTINGS_EVENTS.discarded,
        { world: context.worldName, change_count: Object.keys(context.changes).length },
        context.trackCtx,
      ),
    clearChanges: assign({ changes: () => ({}) }),
    trackSaving: ({ context }) =>
      context.track(
        WORLD_SETTINGS_EVENTS.saving,
        { world: context.worldName, fields: Object.keys(context.changes) },
        context.trackCtx,
      ),
    trackSaved: ({ context }) =>
      context.track(
        WORLD_SETTINGS_EVENTS.saved,
        {
          world: context.worldName,
          fields: context.result?.savedFields ?? Object.keys(context.changes),
          stub: context.result?.stub ?? false,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "worldSettingsWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    worldName: input.worldName ?? "neon-market.dcl.eth",
    save: input.save ?? simulateSave,
    track: input.track ?? defaultTrack,
    changes: {},
  }),
  initial: "details",
  entry: "trackOpened",
  states: {
    details: {
      entry: { type: "trackTabViewed", params: { tab: "details" } },
      on: {
        CHANGE: { actions: ["recordChange", "trackChanged"] },
        NEXT: { target: "layout" },
        GO_TAB: [
          { target: "layout", guard: ({ event }) => event.tab === "layout" },
          { target: "misc", guard: ({ event }) => event.tab === "misc" },
        ],
      },
    },
    layout: {
      entry: { type: "trackTabViewed", params: { tab: "layout" } },
      on: {
        CHANGE: { actions: ["recordChange", "trackChanged"] },
        NEXT: { target: "misc" },
        BACK: { target: "details" },
        GO_TAB: [
          { target: "details", guard: ({ event }) => event.tab === "details" },
          { target: "misc", guard: ({ event }) => event.tab === "misc" },
        ],
      },
    },
    misc: {
      entry: { type: "trackTabViewed", params: { tab: "misc" } },
      on: {
        CHANGE: { actions: ["recordChange", "trackChanged"] },
        REVIEW: { target: "review" },
        BACK: { target: "layout" },
        GO_TAB: [
          { target: "details", guard: ({ event }) => event.tab === "details" },
          { target: "layout", guard: ({ event }) => event.tab === "layout" },
        ],
      },
    },
    review: {
      entry: "trackReviewReached",
      on: {
        SAVE: { target: "saving" },
        BACK: { target: "misc" },
        DISCARD: { target: "details", actions: ["trackDiscarded", "clearChanges"] },
        GO_TAB: [
          { target: "details", guard: ({ event }) => event.tab === "details" },
          { target: "layout", guard: ({ event }) => event.tab === "layout" },
          { target: "misc", guard: ({ event }) => event.tab === "misc" },
        ],
      },
    },
    saving: {
      entry: [assign({ error: undefined }), "trackSaving"],
      invoke: {
        id: "runSave",
        src: "runSave",
        input: ({ context }) => ({
          worldName: context.worldName,
          changes: context.changes,
          save: context.save,
        }),
        onDone: {
          target: "saved",
          actions: [assign({ result: ({ event }) => event.output }), "trackSaved"],
        },
        onError: {
          target: "error",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "save failed"),
          }),
        },
      },
    },
    saved: {
      type: "final",
    },
    error: {
      on: {
        RETRY: { target: "saving" },
      },
    },
  },
});

export type WorldSettingsMachine = typeof worldSettingsMachine;

export function resolveWorldSettingsSnapshot(args: {
  step: WorldSettingsStateId;
  trackCtx: TrackContext;
  worldName?: string;
  save?: SaveFn;
  track?: TrackFn;
  changes?: Record<string, true>;
}) {
  const { step, trackCtx, worldName, save, track, changes } = args;
  if (step === "details") return undefined;
  const seededChanges =
    changes ?? (step === "review" || step === "saving" || step === "saved"
      ? { "details.title": true as const }
      : {});
  const context: WorldSettingsCtx = {
    trackCtx,
    worldName: worldName ?? "neon-market.dcl.eth",
    save: save ?? simulateSave,
    track: track ?? defaultTrack,
    changes: seededChanges,
  };
  return worldSettingsMachine.resolveState({ value: step, context });
}
