export type ClientEvents = {
  cl_backpack_browsed: {
    empty: boolean;
  };
  cl_backpack_color_changed: {
    color: string;
    kind: "eye" | "hair" | "skin";
  };
  cl_backpack_done: Record<string, never>;
  cl_backpack_equipped: {
    slot: string;
    urn: string;
  };
  cl_backpack_inventory_empty: Record<string, never>;
  cl_backpack_opened: Record<string, never>;
  cl_backpack_review_reached: Record<string, never>;
  cl_backpack_saved: {
    count: number;
    deployed: boolean;
    entity_id?: string;
  };
  cl_backpack_selected: {
    category: string;
    rarity: null | string;
    urn: string;
  };
  cl_community_browse_viewed: {
    count: number;
    search: null | string;
    source: "fixture" | "live";
  };
  cl_community_create_opened: Record<string, never>;
  cl_community_create_submit_failed: {
    error?: string;
  };
  cl_community_created: {
    community_id?: string;
    stub: true;
  };
  cl_community_detail_viewed: {
    community_id: string;
    members_count: null | number;
    privacy: "private" | "public";
    source: "fixture" | "live";
  };
  cl_community_gate_passed: {
    had_name: boolean;
  };
  cl_community_gate_viewed: Record<string, never>;
  cl_community_join_started: {
    action?: "join" | "request";
    community_id?: string;
  };
  cl_community_joined: {
    action?: "join" | "request";
    community_id?: string;
    pending: boolean;
    stub: true;
  };
  cl_community_request_submitted: {
    community_id?: string;
  };
  cl_community_review_reached: {
    has_thumbnail: boolean;
    privacy: "private" | "public";
    visibility: "all" | "unlisted";
  };
  cl_community_step_completed: {
    from: "create" | "details" | "done" | "gate" | "profile" | "review" | "submit";
    to: "create" | "details" | "done" | "gate" | "profile" | "review" | "submit";
  };
  cl_community_submit_attempted: {
    privacy: "private" | "public";
    visibility: "all" | "unlisted";
  };
  cl_emotes_assigned: {
    slot?: number;
    urn?: string;
  };
  cl_emotes_browse: {
    slot?: number;
  };
  cl_emotes_done: Record<string, never>;
  cl_emotes_review: {
    count: number;
  };
  cl_emotes_saved: {
    count: number;
    stub: true;
  };
  cl_emotes_slot_picked: {
    slot?: number;
  };
  cl_emotes_started: Record<string, never>;
  cl_explore_opened: {
    place_count: number;
  };
  cl_map_confirm_reached: {
    coords?: string;
    set_home: boolean;
  };
  cl_map_filtered: {
    filter: "all" | "live" | "minigames" | "people" | "poi";
  };
  cl_map_jump: {
    coords?: string;
    jump_url?: string;
    place_id?: string;
    set_home: boolean;
    simulated: true;
  };
  cl_map_jump_done: {
    coords?: string;
    place_id?: string;
  };
  cl_map_opened: {
    filter: "all" | "live" | "minigames" | "people" | "poi";
  };
  cl_map_pin_selected: {
    coords: string;
    place_id: string;
  };
  cl_open_card_clicked: {
    target: "avatar" | "genesis" | "random";
  };
  cl_open_genesis_spawn: {
    place_id: string;
    user_count: number;
  };
  cl_open_jumped_in: {
    place_id: string;
    variant: "base" | "genesis" | "three-cards";
  };
  cl_open_screen_shown: {
    variant: "base" | "genesis" | "three-cards";
  };
  cl_outfit_captured: {
    slot: number;
    wearables: number;
  };
  cl_outfit_named: {
    name: string;
    slot: number;
  };
  cl_outfit_save_completed: {
    slot: number;
  };
  cl_outfit_save_started: {
    slot: number;
  };
  cl_outfit_saved: {
    name: string;
    simulated: true;
    slot: number;
    wearables: number;
  };
  cl_outfit_slot_gated: {
    reason: string;
    slot: number;
  };
  cl_passport_claim_name: {
    address: string;
    simulated: true;
  };
  cl_passport_edit: {
    field: string;
    simulated: true;
  };
  cl_passport_item_clicked: {
    category: string;
    name: string;
  };
  cl_passport_link_clicked: {
    url: string;
  };
  cl_passport_opened: {
    address: string;
    from_fixture: boolean;
    self: boolean;
  };
  cl_passport_photo_opened: {
    photo_id: string;
  };
  cl_passport_profile_empty: {
    address: string;
    from_fixture: boolean;
  };
  cl_passport_tab_viewed: {
    tab: "badges" | "overview" | "photos";
  };
  cl_setting_changed: {
    key: string;
    kind: "dropdown" | "slider" | "toggle";
    tab: "chat" | "controls" | "graphics" | "sounds";
    value: number;
  };
  cl_settings_opened: {
    tab: "chat" | "controls" | "graphics" | "sounds";
  };
  cl_settings_tab_changed: {
    tab: "chat" | "controls" | "graphics" | "sounds";
  };
  cl_voice_join: {
    kind?: "community" | "private";
    room?: string;
  };
  cl_voice_left: {
    kind?: "community" | "private";
    stub: true;
  };
  cl_voice_mute_toggled: {
    muted: boolean;
  };
  cl_voice_session_failed: {
    error?: string;
    kind?: "community" | "private";
  };
  cl_voice_session_requested: {
    kind: "community" | "private";
  };
  cl_voice_token_issued: {
    kind?: "community" | "private";
    stub: true;
  };
  cl_voice_widget_opened: Record<string, never>;
};
