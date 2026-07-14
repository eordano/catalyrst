export type MiscEvents = {
  catalyst_schema_drift: {
    issues?: string[];
    issues_count?: number;
    kind?: string;
    module?: string;
    path?: string;
  };
  client_error: {
    client_ts: string;
    error_message: string;
    error_name: string;
    ua: string;
    url: string;
  };
  experiment_exposed: {
    exp_key?: string;
    variant?: string;
  };
  ffw_shop_landing: {
    item_id: string;
    nid: string;
    on_sale: boolean;
  };
  notif_filter_applied: {
    count: number;
    filter: string;
  };
  notif_mark_all_read: {
    count: number;
  };
  notif_marked_read: {
    id: string;
    type: string;
  };
  notif_panel_opened: {
    count: number;
    read_failed: boolean;
    unread: number;
  };
  ov_friend_action_completed: {
    action?: "accept" | "block" | "cancel" | "reject" | "request";
    address?: string;
    stub: true;
  };
  ov_friend_action_failed: {
    action?: "accept" | "block" | "cancel" | "reject" | "request";
    address?: string;
    error?: string;
  };
  ov_friend_action_started: {
    action?: "accept" | "block" | "cancel" | "reject" | "request";
    address?: string;
  };
  ov_friend_block_confirmed: {
    address?: string;
  };
  ov_friend_block_prompt: {
    address?: string;
  };
  ov_friend_panel_opened: {
    tab: "blocked" | "friends" | "requests";
  };
  page_not_found: {
    path: string;
    referrer: string;
    spa?: true;
    ua: string;
  };
  pl_shop_entry_shown: {
    variant: "pill" | "rail";
  };
  pl_shop_opened: {
    item_id: null | string;
    target: "pill" | "rail_cta" | "rail_item";
    variant: "pill" | "rail";
  };
  place_card_clicked: {
    place_id: string;
  };
  place_list_viewed: {
    category: null | string;
    count: number;
    search: null | string;
  };
  profile_card_clicked: {
    item_id: string;
    tab: "assets" | "communities" | "creations" | "overview" | "photos" | "places" | "referral-rewards";
  };
  profile_communities_load_failed: {
    address: string;
  };
  profile_tab_changed: {
    tab: "assets" | "communities" | "creations" | "overview" | "photos" | "places" | "referral-rewards";
  };
  profile_viewed: {
    address: string;
    has_claimed_name: boolean;
    own: boolean;
    source: "fallback" | "live";
  };
  scene_analytics_exported: {
    scene_id: string;
    scene_type: "genesis" | "world";
  };
  scene_analytics_viewed: {
    scene_id?: string;
    scene_type?: "genesis" | "world";
    source: string;
  };
};
