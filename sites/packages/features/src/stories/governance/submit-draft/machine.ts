import { assign, fromPromise, setup } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type SubmitResult = {
  proposalId: string;
};

export type SubmitFn = (args: {
  pollId: string;
  title: string;
  signal?: AbortSignal;
}) => Promise<SubmitResult>;

export type { TrackFn };

export type CoAuthor = { addr: string };

export type DraftForm = {
  pollId?: string;
  title: string;
  bodies: Record<string, string>;
  coauthors: CoAuthor[];
};

export type DraftInput = {
  trackCtx: TrackContext;
  submitDraft?: SubmitFn;
  track?: TrackFn;
  draft?: Partial<DraftForm>;
};

export type DraftContext = {
  trackCtx: TrackContext;
  submitDraft: SubmitFn;
  track: TrackFn;
  draft: DraftForm;
  result?: SubmitResult;
  error?: string;
};

export type DraftEvent =
  | { type: "CLEAR_GATE"; pollId: string }
  | { type: "SUBMIT_DETAILS"; title: string; bodies: Record<string, string> }
  | { type: "NEXT"; coauthors?: CoAuthor[] }
  | { type: "BACK" }
  | { type: "SUBMIT" }
  | { type: "RETRY" };

export const DRAFT_EVENTS = {
  started: "gv_draft_started",
  detailsCompleted: "gv_draft_details_completed",
  coauthorsSet: "gv_draft_coauthors_set",
  stepAdvanced: "gv_draft_step_advanced",
  submitAttempted: "gv_draft_submit_attempted",
  submitted: "gv_draft_submitted",
} as const;

export const STATE_TO_SLUG = {
  intro: "intro",
  details: "details",
  coauthors: "coauthors",
  review: "review",
  submitting: "submitting",
  submitError: "submit-error",
  success: "success",
} as const;

export type DraftStateId = keyof typeof STATE_TO_SLUG;
export type DraftStepSlug = (typeof STATE_TO_SLUG)[DraftStateId];

export const FIRST_STEP_SLUG: DraftStepSlug = STATE_TO_SLUG.intro;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "intro");

export const SLUG_TO_STATE: Record<DraftStepSlug, DraftStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => DraftStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => DraftStateId = stepSlugs.toState;

export const simulateSubmit: SubmitFn = async ({ pollId, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const slug = (pollId || "draft").slice(0, 8);
  return { proposalId: `stub-draft-${slug}-${Date.now().toString(36)}` };
};

const DEFAULT_DRAFT: DraftForm = { title: "", bodies: {}, coauthors: [] };

export const draftMachine = setup({
  types: {
    context: {} as DraftContext,
    events: {} as DraftEvent,
    input: {} as DraftInput,
  },
  actors: {
    runSubmit: fromPromise<
      SubmitResult,
      { pollId: string; title: string; submitDraft: SubmitFn }
    >(({ input, signal }) =>
      input.submitDraft({ pollId: input.pollId, title: input.title, signal }),
    ),
  },
  actions: {
    setPoll: assign({
      draft: ({ context, event }) =>
        event.type === "CLEAR_GATE"
          ? { ...context.draft, pollId: event.pollId }
          : context.draft,
    }),
    setDetails: assign({
      draft: ({ context, event }) =>
        event.type === "SUBMIT_DETAILS"
          ? { ...context.draft, title: event.title, bodies: event.bodies }
          : context.draft,
    }),
    setCoAuthors: assign({
      draft: ({ context, event }) =>
        event.type === "NEXT" && event.coauthors
          ? { ...context.draft, coauthors: event.coauthors }
          : context.draft,
    }),
    trackStarted: ({ context }) =>
      context.track(
        DRAFT_EVENTS.started,
        { poll_id: context.draft.pollId },
        context.trackCtx,
      ),
    trackDetailsCompleted: ({ context }) =>
      context.track(
        DRAFT_EVENTS.detailsCompleted,
        {
          poll_id: context.draft.pollId,
          title_len: context.draft.title.length,
          bodies: Object.keys(context.draft.bodies).length,
        },
        context.trackCtx,
      ),
    trackCoAuthorsSet: ({ context }) =>
      context.track(
        DRAFT_EVENTS.coauthorsSet,
        { count: context.draft.coauthors.length },
        context.trackCtx,
      ),
    trackStepReview: ({ context }) =>
      context.track(
        DRAFT_EVENTS.stepAdvanced,
        { to: "review", poll_id: context.draft.pollId },
        context.trackCtx,
      ),
    trackSubmitAttempted: ({ context }) =>
      context.track(
        DRAFT_EVENTS.submitAttempted,
        {
          poll_id: context.draft.pollId,
          title_len: context.draft.title.length,
          coauthors: context.draft.coauthors.length,
        },
        context.trackCtx,
      ),
    trackSubmitted: ({ context }) =>
      context.track(
        DRAFT_EVENTS.submitted,
        {
          poll_id: context.draft.pollId,
          proposal_id: context.result?.proposalId,
          simulated: true,
        },
        context.trackCtx,
      ),
  },
}).createMachine({
  id: "draftSubmit",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    submitDraft: input.submitDraft ?? simulateSubmit,
    track: input.track ?? defaultTrack,
    draft: { ...DEFAULT_DRAFT, ...(input.draft ?? {}) },
  }),
  initial: "intro",
  states: {
    intro: {
      on: {
        CLEAR_GATE: {
          target: "details",
          actions: ["setPoll", "trackStarted"],
        },
      },
    },
    details: {
      on: {
        SUBMIT_DETAILS: {
          target: "coauthors",
          actions: ["setDetails", "trackDetailsCompleted"],
        },
        BACK: { target: "intro" },
      },
    },
    coauthors: {
      on: {
        NEXT: {
          target: "review",
          actions: ["setCoAuthors", "trackCoAuthorsSet", "trackStepReview"],
        },
        BACK: { target: "details" },
      },
    },
    review: {
      on: {
        SUBMIT: { target: "submitting", actions: "trackSubmitAttempted" },
        BACK: { target: "coauthors" },
      },
    },
    submitting: {
      entry: assign({ error: undefined }),
      invoke: {
        id: "runSubmit",
        src: "runSubmit",
        input: ({ context }) => ({
          pollId: context.draft.pollId ?? "",
          title: context.draft.title,
          submitDraft: context.submitDraft,
        }),
        onDone: {
          target: "success",
          actions: [
            assign({ result: ({ event }) => event.output }),
            "trackSubmitted",
          ],
        },
        onError: {
          target: "submitError",
          actions: assign({
            error: ({ event }) =>
              toErrorMessage(event.error, "submit failed"),
          }),
        },
      },
    },
    submitError: {
      on: {
        RETRY: { target: "submitting" },
        BACK: { target: "review" },
      },
    },
    success: {
      type: "final",
    },
  },
});

export type DraftMachine = typeof draftMachine;

export function resolveDraftSnapshot(args: {
  step: DraftStateId;
  trackCtx: TrackContext;
  submitDraft?: SubmitFn;
  track?: TrackFn;
  draft?: Partial<DraftForm>;
}) {
  const { step, trackCtx, submitDraft, track, draft } = args;
  if (step === "intro") return undefined;
  const seeded: DraftForm = {
    pollId: "sample-poll",
    title: "Establish a standing Community Grants review committee",
    bodies: {
      summary: "Create a standing committee to review Community Grant requests.",
      abstract: "",
      motivation: "",
      specification: "",
      conclusion: "",
    },
    coauthors: [],
    ...(draft ?? {}),
  };
  const context: DraftContext = {
    trackCtx,
    submitDraft: submitDraft ?? simulateSubmit,
    track: track ?? defaultTrack,
    draft: seeded,
  };
  return draftMachine.resolveState({ value: step, context });
}
