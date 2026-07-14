import { assign, fromPromise, setup, type ErrorActorEvent } from "xstate";
import { toErrorMessage } from "@core/lib/errors";
import { makeStepSlugs } from "@core/lib/stories/step-slugs";

import { track as defaultTrack, type TrackContext, type TrackFn } from "@core/lib/telemetry/track";

export type { TrackFn };

export type SelectedPeriod = {
  index: number;
  minDays: number;
  maxDays: number;
  pricePerDayMana: number;
};

export type RentalCommitResult = {
  rentalId: string;
  txHash: string;
};

export type CommitPhaseFn = (args: {
  phase: "approve" | "sign" | "submit";
  signal?: AbortSignal;
}) => Promise<void>;

export type RentInput = {
  trackCtx: TrackContext;
  commit?: CommitPhaseFn;
  track?: TrackFn;
  rentalId?: string;
  rentalContractAddress?: string;
};

export type RentContext = {
  trackCtx: TrackContext;
  commit: CommitPhaseFn;
  track: TrackFn;
  rentalId: string;
  rentalContractAddress: string;
  period?: SelectedPeriod;
  days?: number;
  totalMana?: number;
  result?: RentalCommitResult;
  error?: string;
};

export type RentEvent =
  | { type: "START" }
  | { type: "SELECT_PERIOD"; period: SelectedPeriod }
  | { type: "SET_DAYS"; days: number }
  | { type: "ACCEPT" }
  | { type: "BACK" }
  | { type: "RETRY" };

export const RENT_EVENTS = {
  started: "mk_rent_started",
  periodSelected: "mk_rent_period_selected",
  priceSet: "mk_rent_price_set",
  manaApproved: "mk_rent_mana_approved",
  signReached: "mk_rent_sign_reached",
  signed: "mk_rent_signed",
  completed: "mk_rent_completed",
  abandoned: "mk_rent_abandoned",
  failed: "mk_rent_failed",
  retried: "mk_rent_retried",
} as const;

export const STATE_TO_SLUG = {
  review: "review-land",
  period: "select-period",
  price: "set-price-or-accept",
  approve: "approve-mana",
  sign: "sign-rental",
  confirm: "confirm",
  success: "success",
  error: "error",
} as const;

export type RentStateId = keyof typeof STATE_TO_SLUG;
export type RentStepSlug = (typeof STATE_TO_SLUG)[RentStateId];

export const FIRST_STEP_SLUG: RentStepSlug = STATE_TO_SLUG.review;

const stepSlugs = makeStepSlugs(STATE_TO_SLUG, "review");

export const SLUG_TO_STATE: Record<RentStepSlug, RentStateId> = stepSlugs.slugToState;

export const stateToSlug: (value: string) => RentStepSlug = stepSlugs.toSlug;

export const slugToState: (slug: string | null | undefined) => RentStateId = stepSlugs.toState;

export const simulatePhase: CommitPhaseFn = async ({ signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 350);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
};

export const rentMachine = setup({
  types: {
    context: {} as RentContext,
    events: {} as RentEvent | ErrorActorEvent,
    input: {} as RentInput,
  },
  actors: {
    runPhase: fromPromise<
      void,
      { phase: "approve" | "sign" | "submit"; commit: CommitPhaseFn }
    >(({ input, signal }) => input.commit({ phase: input.phase, signal })),
  },
  actions: {
    trackStarted: ({ context }) =>
      context.track(RENT_EVENTS.started, {}, context.trackCtx),
    setPeriod: assign({
      period: ({ event }) =>
        event.type === "SELECT_PERIOD" ? event.period : undefined,
      days: ({ event }) =>
        event.type === "SELECT_PERIOD" ? event.period.minDays : undefined,
    }),
    trackPeriodSelected: ({ context }) =>
      context.track(
        RENT_EVENTS.periodSelected,
        {
          period_index: context.period?.index,
          min_days: context.period?.minDays,
          max_days: context.period?.maxDays,
        },
        context.trackCtx,
      ),
    setDays: assign({
      days: ({ context, event }) => {
        if (event.type !== "SET_DAYS" || !context.period) return context.days;
        const { minDays, maxDays } = context.period;
        return Math.min(maxDays, Math.max(minDays, Math.floor(event.days)));
      },
    }),
    finalizeQuote: assign(({ context }) => {
      const period = context.period;
      if (!period) return {};
      const days = context.days ?? period.minDays;
      const clamped = Math.min(period.maxDays, Math.max(period.minDays, days));
      return { days: clamped, totalMana: period.pricePerDayMana * clamped };
    }),
    trackPriceSet: ({ context }) =>
      context.track(
        RENT_EVENTS.priceSet,
        { days: context.days, total_mana: context.totalMana },
        context.trackCtx,
      ),
    trackManaApproved: ({ context }) =>
      context.track(RENT_EVENTS.manaApproved, {}, context.trackCtx),
    trackSignReached: ({ context }) =>
      context.track(RENT_EVENTS.signReached, {}, context.trackCtx),
    trackSigned: ({ context }) =>
      context.track(RENT_EVENTS.signed, {}, context.trackCtx),
    trackCompleted: ({ context }) =>
      context.track(
        RENT_EVENTS.completed,
        {
          rental_id: context.result?.rentalId,
          tx_hash: context.result?.txHash,
          days: context.days,
          total_mana: context.totalMana,
          stub: true,
        },
        context.trackCtx,
      ),
    trackAbandoned: ({ context }) =>
      context.track(RENT_EVENTS.abandoned, {}, context.trackCtx),
    trackFailed: ({ context }) =>
      context.track(
        RENT_EVENTS.failed,
        { error: context.error },
        context.trackCtx,
      ),
    trackRetried: ({ context }) =>
      context.track(RENT_EVENTS.retried, {}, context.trackCtx),
    setError: assign({
      error: ({ event }) => {
        if (!("error" in event)) return undefined;
        return toErrorMessage(event.error, "rental failed");
      },
    }),
    clearError: assign({ error: undefined }),
    setResult: assign({
      result: ({ context }) => ({
        rentalId: context.rentalId,
        txHash: "0xsimulated" + context.rentalId.replace(/[^a-z0-9]/gi, "").slice(0, 24),
      }),
    }),
  },
}).createMachine({
  id: "rentLandWizard",
  context: ({ input }) => ({
    trackCtx: input.trackCtx,
    commit: input.commit ?? simulatePhase,
    track: input.track ?? defaultTrack,
    rentalId: input.rentalId ?? "rental-sim",
    rentalContractAddress:
      input.rentalContractAddress ?? "0x42f4ba48791e2de32f5fbf553441c2672864bb33",
  }),
  initial: "review",
  states: {
    review: {
      on: {
        START: { target: "period", actions: "trackStarted" },
        BACK: { actions: "trackAbandoned" },
      },
    },
    period: {
      on: {
        SELECT_PERIOD: {
          target: "price",
          actions: ["setPeriod", "trackPeriodSelected"],
        },
        BACK: { target: "review" },
      },
    },
    price: {
      on: {
        SET_DAYS: { actions: "setDays" },
        ACCEPT: {
          target: "approve",
          actions: ["finalizeQuote", "trackPriceSet"],
        },
        BACK: { target: "period" },
      },
    },
    approve: {
      entry: ["clearError"],
      invoke: {
        id: "approveMana",
        src: "runPhase",
        input: ({ context }) => ({ phase: "approve" as const, commit: context.commit }),
        onDone: { target: "sign", actions: "trackManaApproved" },
        onError: { target: "error", actions: "setError" },
      },
    },
    sign: {
      entry: "trackSignReached",
      invoke: {
        id: "signRental",
        src: "runPhase",
        input: ({ context }) => ({ phase: "sign" as const, commit: context.commit }),
        onDone: { target: "confirm", actions: "trackSigned" },
        onError: { target: "error", actions: "setError" },
      },
    },
    confirm: {
      invoke: {
        id: "submitRental",
        src: "runPhase",
        input: ({ context }) => ({ phase: "submit" as const, commit: context.commit }),
        onDone: {
          target: "success",
          actions: ["setResult", "trackCompleted"],
        },
        onError: { target: "error", actions: "setError" },
      },
    },
    success: {
      type: "final",
    },
    error: {
      entry: "trackFailed",
      on: {
        RETRY: { target: "approve", actions: "trackRetried" },
      },
    },
  },
});

export type RentMachine = typeof rentMachine;

export function resolveRentSnapshot(args: {
  step: RentStateId;
  trackCtx: TrackContext;
  commit?: CommitPhaseFn;
  track?: TrackFn;
  rentalId?: string;
  rentalContractAddress?: string;
  period?: SelectedPeriod;
  days?: number;
}) {
  const {
    step,
    trackCtx,
    commit,
    track,
    rentalId = "rental-sim",
    rentalContractAddress = "0x42f4ba48791e2de32f5fbf553441c2672864bb33",
    period,
    days,
  } = args;
  if (step === "review") return undefined;

  const seededPeriod: SelectedPeriod | undefined =
    period ??
    (step === "period"
      ? undefined
      : { index: 0, minDays: 1, maxDays: 6, pricePerDayMana: 100 });
  const seededDays = days ?? seededPeriod?.minDays;
  const context: RentContext = {
    trackCtx,
    commit: commit ?? simulatePhase,
    track: track ?? defaultTrack,
    rentalId,
    rentalContractAddress,
    period: seededPeriod,
    days: seededDays,
    totalMana:
      seededPeriod && seededDays ? seededPeriod.pricePerDayMana * seededDays : undefined,
  };
  return rentMachine.resolveState({ value: step, context });
}
