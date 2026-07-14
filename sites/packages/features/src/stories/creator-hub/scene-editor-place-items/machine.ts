import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type PlacedEntity = {
  entity: number;
  assetId: string;
  assetName: string;
};

export type SaveResult = {
  composite: string;
  entities: number;
  simulated?: boolean;
  via?: "fsa-handle" | "download" | "canceled";
};

export type SaveFn = (args: {
  placed?: PlacedEntity;
  component?: string;
  selected?: SelectedEntity;
  modifiedName?: string;
  deleted?: boolean;
  signal?: AbortSignal;
}) => Promise<SaveResult>;

export type EditorInput = {
  trackCtx: TrackContext;
  save?: SaveFn;
  track?: TrackFn;
};

export type SelectedEntity = { entity: number; name: string };

export type EditorContext = {
  trackCtx: TrackContext;
  save: SaveFn;
  track: TrackFn;
  query?: string;
  pendingAsset?: { id: string; name: string };
  placed?: PlacedEntity;
  transformAxes: string[];
  component?: string;
  selected?: SelectedEntity;
  modifiedName?: string;
  deleted?: boolean;
  result?: SaveResult;
  error?: string;
};

export type EditorEvent =
  | { type: "BROWSE_ASSETS" }
  | { type: "SEARCH"; query: string }
  | { type: "SELECT_ASSET"; id: string; name: string }
  | { type: "PLACE_ASSET"; id: string; name: string }
  | { type: "CONFIRM_ENTITY"; entity: number }
  | { type: "SET_TRANSFORM"; axis: string }
  | { type: "ADD_COMPONENT_PANEL" }
  | { type: "SAVE_WITHOUT_COMPONENT" }
  | { type: "PICK_COMPONENT"; component: string }
  | { type: "MODIFY_ENTITY"; entity: number; name: string }
  | { type: "RENAME_ENTITY"; name: string }
  | { type: "ADD_COMPONENT"; component: string }
  | { type: "DELETE_ENTITY" }
  | { type: "SAVE_EDIT" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const EDITOR_EVENTS = {
  opened: "ch_editor_opened",
  assetsBrowsed: "ch_editor_assets_browsed",
  assetSearched: "ch_editor_asset_searched",
  entityCreated: "ch_editor_entity_created",
  transformSet: "ch_editor_transform_set",
  componentAdded: "ch_editor_component_added",
  entityModified: "ch_editor_entity_modified",
  entityRenamed: "ch_editor_entity_renamed",
  entityDeleted: "ch_editor_entity_deleted",
  saved: "ch_editor_saved",
} as const;

export const STATE_TO_SLUG = {
  editing: "open",
  browsingAssets: "assets",
  placing: "place",
  transforming: "transform",
  addingComponent: "component",
  modifying: "modify",
  saving: "save",
  saved: "saved",
  error: "error",
} as const;

export type EditorStateId = keyof typeof STATE_TO_SLUG;
export type EditorStepSlug = (typeof STATE_TO_SLUG)[EditorStateId];

export const FIRST_STEP_SLUG: EditorStepSlug = STATE_TO_SLUG.editing;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "editing");

export const SLUG_TO_STATE: Record<EditorStepSlug, EditorStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => EditorStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => EditorStateId = stepSlugs.toState;

export const simulateSave: SaveFn = async ({ placed, component, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 350);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  return {
    composite: "main.composite",
    entities: 5 + (placed ? 1 : 0) + (component ? 0 : 0),
    simulated: true,
  };
};

export const editorMachine = setup({
  types: {
    context: {} as EditorContext,
    events: {} as EditorEvent,
    input: {} as EditorInput,
  },
  actors: {
    runSave: fromPromise<
      SaveResult,
      {
        placed?: PlacedEntity;
        component?: string;
        selected?: SelectedEntity;
        modifiedName?: string;
        deleted?: boolean;
        save: SaveFn;
      }
    >(({ input, signal }) =>
      input.save({
        placed: input.placed,
        component: input.component,
        selected: input.selected,
        modifiedName: input.modifiedName,
        deleted: input.deleted,
        signal,
      }),
    ),
  },
  actions: {
    trackOpened: ({ context }) =>
      context.track(EDITOR_EVENTS.opened, {}, context.trackCtx),
    trackAssetsBrowsed: ({ context }) =>
      context.track(EDITOR_EVENTS.assetsBrowsed, {}, context.trackCtx),
    setQuery: assign({
      query: ({ event }) => (event.type === "SEARCH" ? event.query : undefined),
    }),
    trackAssetSearched: ({ context, event }) => {
      if (event.type !== "SEARCH") return;
      context.track(
        EDITOR_EVENTS.assetSearched,
        { query: event.query },
        context.trackCtx,
      );
    },
    setPendingAsset: assign({
      pendingAsset: ({ event }) =>
        event.type === "PLACE_ASSET" || event.type === "SELECT_ASSET"
          ? { id: event.id, name: event.name }
          : undefined,
    }),
    setPlaced: assign({
      placed: ({ context, event }) =>
        event.type === "CONFIRM_ENTITY" && context.pendingAsset
          ? {
              entity: event.entity,
              assetId: context.pendingAsset.id,
              assetName: context.pendingAsset.name,
            }
          : context.placed,
    }),
    trackEntityCreated: ({ context }) =>
      context.track(
        EDITOR_EVENTS.entityCreated,
        {
          asset_id: context.placed?.assetId,
          asset_name: context.placed?.assetName,
          entity: context.placed?.entity,
        },
        context.trackCtx,
      ),
    recordTransformAxis: assign({
      transformAxes: ({ context, event }) => {
        if (event.type !== "SET_TRANSFORM") return context.transformAxes;
        return context.transformAxes.includes(event.axis)
          ? context.transformAxes
          : [...context.transformAxes, event.axis];
      },
    }),
    trackTransformSet: ({ context, event }) => {
      if (event.type !== "SET_TRANSFORM") return;
      context.track(
        EDITOR_EVENTS.transformSet,
        { axis: event.axis },
        context.trackCtx,
      );
    },
    setComponent: assign({
      component: ({ event }) =>
        event.type === "PICK_COMPONENT" ? event.component : undefined,
    }),
    trackComponentAdded: ({ context, event }) => {
      if (event.type !== "PICK_COMPONENT" && event.type !== "ADD_COMPONENT") return;
      context.track(
        EDITOR_EVENTS.componentAdded,
        { component: event.component, on: context.selected?.entity },
        context.trackCtx,
      );
    },
    setSelected: assign({
      selected: ({ context, event }) =>
        event.type === "MODIFY_ENTITY"
          ? { entity: event.entity, name: event.name }
          : context.selected,
      modifiedName: ({ context, event }) =>
        event.type === "MODIFY_ENTITY" ? event.name : context.modifiedName,
      transformAxes: () => [],
      deleted: () => false,
    }),
    trackEntityModified: ({ context }) =>
      context.track(
        EDITOR_EVENTS.entityModified,
        { entity: context.selected?.entity, name: context.selected?.name },
        context.trackCtx,
      ),
    setModifiedName: assign({
      modifiedName: ({ context, event }) =>
        event.type === "RENAME_ENTITY" ? event.name : context.modifiedName,
    }),
    trackEntityRenamed: ({ context, event }) => {
      if (event.type !== "RENAME_ENTITY") return;
      context.track(
        EDITOR_EVENTS.entityRenamed,
        { entity: context.selected?.entity, name: event.name },
        context.trackCtx,
      );
    },
    setModifyComponent: assign({
      component: ({ context, event }) =>
        event.type === "ADD_COMPONENT" ? event.component : context.component,
    }),
    markDeleted: assign({ deleted: () => true }),
    trackEntityDeleted: ({ context }) =>
      context.track(
        EDITOR_EVENTS.entityDeleted,
        { entity: context.selected?.entity, name: context.selected?.name },
        context.trackCtx,
      ),
    trackSaved: ({ context }) =>
      context.track(
        EDITOR_EVENTS.saved,
        {
          mode: context.placed ? "place" : context.selected ? "modify" : "scene",
          entity: context.placed?.entity ?? context.selected?.entity,
          component: context.component,
          renamed: context.modifiedName,
          deleted: context.deleted ?? false,
          composite: context.result?.composite,
          entities: context.result?.entities,
          via: context.result?.via,
          stub: context.result?.simulated === true,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "sceneEditorPlaceItems",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    save: input.save ?? simulateSave,
    track: input.track ?? defaultTrack,
    transformAxes: [],
  }),
  initial: "editing",
  states: {
    editing: {
      entry: "trackOpened",
      on: {
        BROWSE_ASSETS: { target: "browsingAssets", actions: "trackAssetsBrowsed" },
        MODIFY_ENTITY: { target: "modifying", actions: ["setSelected", "trackEntityModified"] },
      },
    },
    browsingAssets: {
      on: {
        SEARCH: { actions: ["setQuery", "trackAssetSearched"] },
        SELECT_ASSET: { actions: "setPendingAsset" },
        PLACE_ASSET: { target: "placing", actions: "setPendingAsset" },
        BACK: { target: "editing" },
      },
    },
    placing: {
      on: {
        CONFIRM_ENTITY: {
          target: "transforming",
          actions: ["setPlaced", "trackEntityCreated"],
        },
        BACK: { target: "browsingAssets" },
      },
    },
    transforming: {
      on: {
        SET_TRANSFORM: { actions: ["recordTransformAxis", "trackTransformSet"] },
        ADD_COMPONENT_PANEL: { target: "addingComponent" },
        SAVE_WITHOUT_COMPONENT: { target: "saving" },
        BACK: { target: "browsingAssets" },
      },
    },
    addingComponent: {
      on: {
        PICK_COMPONENT: {
          target: "saving",
          actions: ["setComponent", "trackComponentAdded"],
        },
        BACK: { target: "transforming" },
      },
    },
    modifying: {
      on: {
        RENAME_ENTITY: { actions: ["setModifiedName", "trackEntityRenamed"] },
        SET_TRANSFORM: { actions: ["recordTransformAxis", "trackTransformSet"] },
        ADD_COMPONENT: { actions: ["setModifyComponent", "trackComponentAdded"] },
        DELETE_ENTITY: { target: "saving", actions: ["markDeleted", "trackEntityDeleted"] },
        SAVE_EDIT: { target: "saving" },
        BACK: { target: "editing" },
      },
    },
    saving: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSave",
        src: "runSave",
        input: ({ context }) => ({
          placed: context.placed,
          component: context.component,
          selected: context.selected,
          modifiedName: context.modifiedName,
          deleted: context.deleted,
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

export type EditorMachine = typeof editorMachine;

export function resolveEditorSnapshot(args: {
  step: EditorStateId;
  trackCtx: TrackContext;
  save?: SaveFn;
  track?: TrackFn;
  asset?: { id: string; name: string };
  selected?: SelectedEntity;
}) {
  const { step, trackCtx, save, track, asset, selected } = args;

  const needsAsset =
    step === "placing" ||
    step === "transforming" ||
    step === "addingComponent" ||
    step === "saving" ||
    step === "saved" ||
    step === "error";
  const needsComponent =
    step === "saving" || step === "saved" || step === "error";
  const isModify = step === "modifying";

  const context: EditorContext = {
    trackCtx,
    save: save ?? simulateSave,
    track: track ?? defaultTrack,
    transformAxes: [],
    pendingAsset: needsAsset ? asset : undefined,
    placed:
      needsAsset && asset
        ? { entity: 540, assetId: asset.id, assetName: asset.name }
        : undefined,
    component: needsComponent ? "MeshCollider" : undefined,
    selected: isModify ? selected : undefined,
    modifiedName: isModify ? selected?.name : undefined,
  };
  return editorMachine.resolveState({ value: step, context });
}
