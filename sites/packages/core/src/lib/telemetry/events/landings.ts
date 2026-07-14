export type LandingsEvents = {
  cast_devices_selected: {
    camera: string;
    mic: string;
    speaker: string;
  };
  cast_ended: {
    stub: true;
  };
  cast_ending: Record<string, never>;
  cast_invalid_token: {
    reason: string;
  };
  cast_join_requested: Record<string, never>;
  cast_not_found_go_home: {
    from: "streamer" | "unknown" | "watcher";
    reason: "ended" | "expired" | "malformed" | "missing";
  };
  cast_not_found_shown: {
    from: "streamer" | "unknown" | "watcher";
    reason: "ended" | "expired" | "malformed" | "missing";
  };
  cast_not_found_view_docs: {
    from: "streamer" | "unknown" | "watcher";
    reason: "ended" | "expired" | "malformed" | "missing";
  };
  cast_permissions_denied: Record<string, never>;
  cast_permissions_granted: Record<string, never>;
  cast_preview_ready: Record<string, never>;
  cast_screenshare_failed: {
    room_id?: string;
    stub: true;
  };
  cast_screenshare_started: {
    room_id?: string;
    stub: true;
  };
  cast_token_checked: Record<string, never>;
  cast_token_valid: {
    is_world?: boolean;
    place_name?: string;
  };
  cast_watch_access_expired: {
    location: string;
  };
  cast_watch_chat_opened: Record<string, never>;
  cast_watch_joined: {
    place_name: string;
    room_id: string;
    stub: true;
  };
  cast_watch_left: Record<string, never>;
  cast_watch_muted: Record<string, never>;
  cast_watch_no_stream: {
    location: string;
  };
  cast_watch_opened: {
    is_world: boolean;
    location: string;
  };
  cast_watch_unmuted: Record<string, never>;
  cast_went_live: {
    room_id?: string;
    stub: true;
  };
  jump_in_completed: {
    launch_url?: string;
    place_id: string;
  };
  jump_in_confirmed: {
    place_id: string;
  };
  jump_in_failed: {
    error?: string;
    place_id: string;
  };
  jump_in_started: {
    confirm_step: boolean;
    place_id: string;
  };
  landing_story_viewed: {
    audience: string;
    kind: "creator" | "user";
    requested?: string;
    via?: "random" | "sticky" | "utm";
  };
  landings_subscription_edited: {
    enabled: boolean;
    notification_type: string;
  };
  landings_subscription_error: {
    kind: "subscribe" | "unsubscribe";
    message?: string;
  };
  landings_subscription_signed_in: Record<string, never>;
  landings_subscription_signin_required: Record<string, never>;
  landings_subscription_started: Record<string, never>;
  landings_subscription_submitting: {
    enabled_count: number;
  };
  landings_subscription_subscribed: {
    enabled_count: number;
  };
  landings_subscription_unsubscribed: Record<string, never>;
  landings_subscription_unsubscribing: Record<string, never>;
  lp_blog_post_clicked: {
    slug: string;
  };
  lp_blog_post_viewed: {
    slug: string;
  };
  lp_blog_shop_entry_shown: {
    variant: "card" | "rail";
  };
  lp_blog_shop_opened: {
    item_id: null | string;
    target: "card" | "rail_cta" | "rail_item";
    variant: "card" | "rail";
  };
  lp_blog_viewed: {
    post_count: number;
  };
  lp_community_created: {
    community_id?: string;
    stub: true;
  };
  lp_community_gate_viewed: Record<string, never>;
  lp_community_join_intent: {
    community_id: string;
    intent: string;
    privacy: "private" | "public";
  };
  lp_community_private_gated: {
    community_id: string;
  };
  lp_community_review_reached: {
    places: number;
    privacy: "private" | "public";
    visibility: "all" | "unlisted";
  };
  lp_community_started: Record<string, never>;
  lp_community_step_completed: {
    from: "basics" | "created" | "places" | "privacy" | "review" | "signinGate" | "submitting" | "thumbnail";
    to: "basics" | "created" | "places" | "privacy" | "review" | "signinGate" | "submitting" | "thumbnail";
  };
  lp_community_submit_attempted: {
    privacy: "private" | "public";
    visibility: "all" | "unlisted";
  };
  lp_community_submit_failed: {
    error?: string;
  };
  lp_community_tab_changed: {
    community_id: string;
    tab: "events" | "members";
  };
  lp_community_viewed: {
    community_id: string;
    members_count: null | number;
    privacy: "private" | "public";
    source: "fixture" | "live";
  };
  lp_creatorhub_download_clicked: {
    arch: string;
    file_name: null | string;
    kind: string;
    os: "macos" | "windows";
  };
  lp_creatorhub_download_success: {
    os: "macos" | "windows";
  };
  lp_creatorhub_download_viewed: {
    arch: string;
    os: "macos" | "windows";
    overridden: boolean;
    version: string;
  };
  lp_discover_viewed: Record<string, never>;
  lp_event_card_clicked: {
    event_id: string;
  };
  lp_event_jump_in: {
    event_id: string;
    position: null | string;
  };
  lp_event_viewed: {
    event_id: string;
    live: boolean;
  };
  lp_hangout_preview_opened: Record<string, never>;
  lp_hangout_signin_gate_viewed: Record<string, never>;
  lp_hangout_started: Record<string, never>;
  lp_hangout_step_completed: {
    from: "cover" | "details" | "location" | "preview" | "review" | "schedule" | "signinGate" | "submitted" | "submitting";
    to: "cover" | "details" | "location" | "preview" | "review" | "schedule" | "signinGate" | "submitted" | "submitting";
  };
  lp_hangout_submit_attempted: {
    location: "land" | "world";
    recurrent: boolean;
  };
  lp_hangout_submit_failed: {
    error?: string;
  };
  lp_hangout_submitted: {
    approved?: boolean;
    event_id?: string;
    stub: true;
  };
  lp_home_download_clicked: {
    placement: string;
    store: string;
  };
  lp_home_rail_clicked: {
    rail: string;
    target: string;
  };
  lp_home_rail_viewed: {
    rail: string;
  };
  lp_home_shop_entry_shown: {
    variant: "cta" | "rail";
  };
  lp_home_shop_opened: {
    item_id: null | string;
    target: "cta" | "rail_cta" | "rail_item";
    variant: "cta" | "rail";
  };
  lp_home_viewed: {
    live_rail: boolean;
    rails: number;
  };
  lp_invite_referrer_resolved: {
    referrer_handle: string;
    resolved: boolean;
    source: string;
  };
  lp_invite_rewards_viewed: {
    accepted: number;
    current_tier: number;
  };
  lp_invite_viewed: {
    has_referrer: boolean;
    referrer_handle: null | string;
    resolved: boolean;
  };
  lp_legal_section_clicked: {
    doc: "brand" | "content" | "ethics" | "privacy" | "referral" | "rewards" | "security" | "terms";
    section: string;
  };
  lp_legal_viewed: {
    doc: "brand" | "content" | "ethics" | "privacy" | "referral" | "rewards" | "security" | "terms";
  };
  lp_rsvp_cancelled: {
    count: number;
    event_id: string;
    stub: true;
  };
  lp_rsvp_cancelling: {
    event_id: string;
  };
  lp_rsvp_confirmed: {
    event_id: string;
  };
  lp_rsvp_error: {
    event_id: string;
    reason: string;
  };
  lp_rsvp_going: {
    count: number;
    event_id: string;
    stub: true;
  };
  lp_rsvp_signin: {
    event_id: string;
    simulated: true;
  };
  lp_rsvp_started: {
    event_id: string;
  };
  lp_rsvp_submitting: {
    event_id: string;
  };
  lp_schedule_created: {
    active?: boolean;
    editing: boolean;
    schedule_id?: string;
    stub: true;
  };
  lp_schedule_gate_viewed: Record<string, never>;
  lp_schedule_list_viewed: {
    count: number;
    source: "empty" | "error" | "live";
  };
  lp_schedule_review_reached: Record<string, never>;
  lp_schedule_started: {
    editing: boolean;
  };
  lp_schedule_step_completed: {
    from: "authGate" | "basics" | "created" | "dates" | "review" | "submitting";
    to: "authGate" | "basics" | "created" | "dates" | "review" | "submitting";
  };
  lp_schedule_submit_attempted: {
    editing: boolean;
    theme: null | string;
  };
  lp_schedule_submit_failed: {
    error?: string;
  };
  lp_support_viewed: Record<string, never>;
  lp_whatson_admin_authenticated: {
    simulated_bearer: true;
  };
  lp_whatson_admin_decision_made: {
    action: "approve" | "archive" | "feature" | "reject" | "unfeature";
    event_id?: string;
  };
  lp_whatson_admin_event_opened: {
    event_id: string;
  };
  lp_whatson_admin_gate_viewed: Record<string, never>;
  lp_whatson_admin_moderated: {
    action?: "approve" | "archive" | "feature" | "reject" | "unfeature";
    event_id?: string;
    stub: true;
  };
  lp_whatson_admin_moderation_confirmed: {
    action?: "approve" | "archive" | "feature" | "reject" | "unfeature";
    event_id?: string;
  };
  lp_whatson_admin_moderation_failed: {
    action?: "approve" | "archive" | "feature" | "reject" | "unfeature";
    error?: string;
    event_id?: string;
  };
  lp_whatson_admin_queue_viewed: Record<string, never>;
  lp_whatson_shop_entry_shown: {
    variant: "pill" | "rail";
  };
  lp_whatson_shop_opened: {
    item_id: null | string;
    target: "pill" | "rail_cta" | "rail_item";
    variant: "pill" | "rail";
  };
  lp_whatson_viewed: {
    filter: null | string;
    live_count: number;
    search: null | string;
    upcoming_count: number;
  };
  report_category_set: {
    reason: "cheating" | "harassment" | "illegal_content" | "impersonation" | "scam_phishing";
  };
  report_completed: {
    evidence_count: number;
    reason: "" | "cheating" | "harassment" | "illegal_content" | "impersonation" | "scam_phishing";
    report_id?: string;
  };
  report_details_set: {
    description_len: number;
  };
  report_evidence_added: {
    file_count: number;
  };
  report_failed: {
    reason?: string;
  };
  report_review_reached: Record<string, never>;
  report_started: Record<string, never>;
  report_submit_started: {
    evidence_count: number;
    reason: "" | "cheating" | "harassment" | "illegal_content" | "impersonation" | "scam_phishing";
  };
  report_target_set: {
    has_reporter: boolean;
  };
  report_validation_failed: {
    fields: string[];
    step: string;
  };
};
