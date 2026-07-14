export type OperatorAdminEvents = {
  admin_community_authenticated: {
    simulated_moderator: true;
  };
  admin_community_decision_selected: {
    community_id?: string;
    decision: "suspend" | "unsuspend";
  };
  admin_community_gate_viewed: Record<string, never>;
  admin_community_list_viewed: {
    status_filter: "active" | "all" | "inactive" | "suspended";
    total: number;
  };
  admin_community_moderation_failed: {
    community_id?: string;
    error?: string;
  };
  admin_community_reviewed: {
    community_id: string;
  };
  admin_community_suspension_committed: {
    community_id?: string;
    has_reason: boolean;
    suspended: boolean;
  };
  admin_debug_console_viewed: {
    panel: "Admin" | "Debug";
  };
  admin_debug_panel_switched: {
    panel: "Admin" | "Debug";
  };
  admin_metrics_surface_clicked: {
    surface: "communities" | "events" | "places";
  };
  admin_metrics_viewed: {
    live_tiles: number;
    unavailable_tiles: number;
  };
  admin_ops_viewed: {
    deployments: number;
    experiments: number;
    probes_ok: number;
    probes_total: number;
    units_active: number;
    units_total: number;
  };
  admin_place_decision_selected: {
    decision: "action" | "dismiss" | "reopen" | "resolve";
    report_id?: string;
  };
  admin_place_disable_toggled: {
    disabled: boolean;
    place_id: null | string;
  };
  admin_place_moderation_committed: {
    decision?: "action" | "dismiss" | "reopen" | "resolve";
    place_disabled: boolean;
    report_id?: string;
  };
  admin_place_moderation_failed: {
    reason?: string;
    report_id?: string;
  };
  admin_place_queue_viewed: {
    open_count: number;
    total: number;
  };
  admin_place_report_opened: {
    entity_id: null | string;
    report_id: string;
  };
  admin_users_unavailable_viewed: {
    reason: string;
  };
  operator_admin_action_failed: {
    action?: "add" | "revoke";
    place_id?: string;
  };
  operator_admin_grant_committed: {
    place_id?: string;
  };
  operator_admin_grant_started: {
    action?: "add" | "revoke";
    place_id?: string;
  };
  operator_admin_revoke_committed: {
    can_be_removed: boolean;
    place_id?: string;
  };
  operator_control_unavailable: {
    control: string;
    reason: string;
  };
  operator_dashboard_funnel_clicked: {
    target: string;
  };
  operator_dashboard_moderation_link: {
    place_id: string;
    target: "scene-admins" | "scene-bans";
  };
  operator_dashboard_range_changed: {
    range: "1h" | "24h" | "6h";
  };
  operator_dashboard_viewed: {
    place_count?: number;
    source?: "catalyst" | "unavailable";
    total_live_players?: number;
  };
  operator_deploy_completed: {
    name?: string;
    target: "land" | "world";
  };
  operator_deploy_started: {
    target: "land" | "world";
  };
  operator_place_card_clicked: {
    place_id: string;
  };
  operator_placement_rejected: {
    phase?: string;
    reason: string;
    target: "land" | "world";
    total_bytes?: number;
  };
  operator_placement_validated: {
    name?: string;
    parcels?: number;
    target: "land" | "world";
  };
  operator_scene_ban_committed: {
    address?: string;
    place_id: string;
    simulated: false;
  };
  operator_scene_ban_failed: {
    action?: "ban" | "unban";
    place_id: string;
  };
  operator_scene_ban_started: {
    action?: "ban" | "unban";
    place_id: string;
  };
  operator_scene_bans_viewed: {
    place_id: string;
    total: number;
  };
  operator_scene_unban_committed: {
    address?: string;
    place_id: string;
    simulated: false;
  };
  operator_user_action_selected: {
    action: "ban" | "unban" | "warn";
  };
  operator_user_ban_committed: {
    has_duration: boolean;
  };
  operator_user_ban_failed: {
    action?: "ban" | "unban" | "warn";
    reason?: "already_banned" | "no_active_ban";
  };
  operator_user_ban_lookup: {
    is_banned: boolean;
  };
  operator_user_bans_viewed: {
    active_count: number;
  };
  operator_user_unban_committed: Record<string, never>;
  operator_user_warning_committed: Record<string, never>;
  operator_visits_viewed: {
    peers: null | number;
    scenes: null | number;
    worlds: null | number;
  };
};
