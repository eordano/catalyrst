import { track, type TrackContext, type TrackFn } from "./track";
import type { TelemetryEventName, TelemetryEvents } from "./events";

export const OPERATOR_EVENTS = {
  deployStarted: "operator_deploy_started",
  placementValidated: "operator_placement_validated",
  placementRejected: "operator_placement_rejected",
  deployCompleted: "operator_deploy_completed",
  sceneAdminOpened: "operator_scene_admin_opened",
  adminChanged: "operator_admin_changed",
  banIssued: "operator_ban_issued",
  dashboardViewed: "operator_dashboard_viewed",
  visitsViewed: "operator_visits_viewed",
  dashboardFunnelClicked: "operator_dashboard_funnel_clicked",
} as const;

export type OperatorTarget = "land" | "world";

export type OperatorEvent = (typeof OPERATOR_EVENTS)[keyof typeof OPERATOR_EVENTS] &
  TelemetryEventName;

export function trackOperator<K extends OperatorEvent>(
  event: K,
  target: OperatorTarget,
  props: Omit<TelemetryEvents[K], "target">,
  ctx: TrackContext,
  emit: TrackFn = track,
): void {
  emit(event, { target, ...props } as TelemetryEvents[K], ctx);
}
