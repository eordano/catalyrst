import { track as defaultTrack, type TrackContext, type TrackFn } from "./track";
import type { TelemetryEvents } from "./events";

export const CREATOR_FUNNEL_STORY = "creator-hub/metrics";

export const CREATOR_FUNNEL_EVENTS = [
  "creator_collection_viewed",
  "creator_item_edited",
  "creator_publish_started",
  "creator_curation_submitted",
  "creator_collection_published",
  "creator_item_listed",
  "creator_store_viewed",
  "creator_sale_completed",
] as const;

export type CreatorFunnelEvent = (typeof CREATOR_FUNNEL_EVENTS)[number];

export const CREATOR_DASHBOARD_VIEWED = "creator_dashboard_viewed";

export function funnelStep(event: CreatorFunnelEvent): number {
  return CREATOR_FUNNEL_EVENTS.indexOf(event);
}

const SCREEN_TO_FUNNEL: Record<string, CreatorFunnelEvent> = {
  bd_item_editor_opened: "creator_item_edited",
  bd_item_editor_saved: "creator_item_edited",
  bd_publish_collection_started: "creator_publish_started",
  bd_publish_collection_submitted: "creator_collection_published",
  bd_curation_decided: "creator_curation_submitted",
  mk_collection_viewed: "creator_collection_viewed",
  mk_sell_completed: "creator_item_listed",
  mk_buy_completed: "creator_sale_completed",
};

export type { TrackFn };

export function withCreatorFunnel(
  base: TrackFn = defaultTrack,
  rollupCtx?: Partial<TrackContext>,
): TrackFn {
  return (event, props, ctx) => {
    base(event, props, ctx);

    const mapped = SCREEN_TO_FUNNEL[event];
    if (!mapped) return;
    try {
      base(
        mapped,
        {
          ...props,
          funnel_step: funnelStep(mapped),
          source_event: event,
        } as TelemetryEvents[CreatorFunnelEvent],
        {
          ...ctx,
          ...rollupCtx,
          sid: rollupCtx?.sid ?? ctx.sid,
          story: CREATOR_FUNNEL_STORY,
        },
      );
    } catch {
    }
  };
}

export function trackCreatorFunnel<K extends CreatorFunnelEvent>(
  event: K,
  props: Omit<TelemetryEvents[K], "funnel_step" | "source_event">,
  ctx: TrackContext,
  base: TrackFn = defaultTrack,
): void {
  try {
    base(
      event,
      { ...props, funnel_step: funnelStep(event) } as TelemetryEvents[K],
      { ...ctx, story: CREATOR_FUNNEL_STORY },
    );
  } catch {
  }
}
