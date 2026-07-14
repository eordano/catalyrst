export type FoundryEvents = {
  fd_appeal_filed: {
    subject_kind: "request" | "role_grant" | "session_series";
  };
  fd_appeal_resolved: {
    verdict: "declined" | "upheld";
  };
  fd_appeal_withdrawn: {
    appeal_id: string;
  };
  fd_bench_report_viewed: {
    slug: string;
    verdict: "fail" | "none" | "pass";
  };
  fd_bundle_downloaded: {
    scene_id: string;
  };
  fd_carry_minted: {
    replaced: boolean;
  };
  fd_carry_redeemed: Record<string, never>;
  fd_consent_changed: {
    state: "granted" | "withdrawn";
    topic: "roster-listing" | "steward-code";
  };
  fd_console_module_viewed: {
    module: "bench" | "costs" | "pipelines" | "trajectories";
  };
  fd_continuity_viewed: Record<string, never>;
  fd_copilot_opened: {
    online: boolean;
  };
  fd_copilot_viewed: {
    online: boolean;
    sessions_total: number;
    tokens_total: number;
  };
  fd_costs_viewed: {
    messages: number;
    tokens_total: number;
  };
  fd_deck_viewed: Record<string, never>;
  fd_embed_started: {
    reachable: boolean;
    slug: string;
  };
  fd_exchange_viewed: {
    pledges_total: number;
    requests_open: number;
  };
  fd_game_link_opened: {
    slug: string;
    target: "editor" | "play" | "world";
  };
  fd_game_viewed: {
    slug: string;
  };
  fd_gdd_approved: {
    doc_id: string;
  };
  fd_gdd_edited: {
    doc_id: string;
    new_id: string;
    section: null | number;
  };
  fd_gdd_list_viewed: {
    docs: number;
  };
  fd_gdd_play_opened: {
    doc_id: string;
    tier: "play" | "same-concept" | "take-on";
  };
  fd_gdd_published: {
    doc_id: string;
    sections: number;
  };
  fd_gdd_version_opened: {
    doc_id: string;
    version: number;
  };
  fd_gdd_viewed: {
    doc_id: string;
    hypotheses: number;
    kind: "brief" | "feature-design" | "proposal" | "shortgdd";
    open_sections: number;
  };
  fd_history_node_opened: {
    kind: "built" | "designed" | "live" | "played" | "reading" | "upcoming";
    slug: string;
  };
  fd_home_viewed: {
    bench_runs: number;
    copilot_online: boolean;
    gdd_total: number;
    scenes_total: number;
  };
  fd_invite_minted: {
    role: "create" | "host" | "start";
  };
  fd_invite_redeemed: {
    role: "admin" | "create" | "host" | "start";
  };
  fd_people_viewed: {
    listed: number;
  };
  fd_person_viewed: {
    name_len: number;
  };
  fd_persona_saved: {
    name_len: number;
  };
  fd_persona_viewed: {
    claimed: boolean;
  };
  fd_pipeline_viewed: {
    slug: string;
  };
  fd_play_viewed: {
    scenes_total: number;
  };
  fd_pledge_retracted: {
    pledges_after: number;
    request_id: string;
  };
  fd_pledge_submitted: {
    pledges_after: number;
    request_id: string;
  };
  fd_replay_opened: {
    events: number;
    trajectory_id: string;
  };
  fd_replay_scrubbed: {
    seq: number;
    trajectory_id: string;
  };
  fd_replay_stepped: {
    direction: "back" | "forward";
    trajectory_id: string;
  };
  fd_request_edited: {
    request_id: string;
    title_len: number;
  };
  fd_request_moderated: {
    request_id: string;
    verdict: "approved" | "closed";
  };
  fd_request_submitted: {
    request_id: string;
    title_len: number;
  };
  fd_response_viewed: {
    slug: string;
  };
  fd_role_chosen: {
    destination: "console" | "copilot" | "play";
    role: "admin" | "create" | "start";
  };
  fd_room_joined: {
    others: number;
    path: string;
  };
  fd_room_message_sent: {
    others: number;
    path: string;
  };
  fd_room_mic_on: {
    others: number;
    path: string;
  };
  fd_scene_note_added: {
    slug: string;
  };
  fd_scene_registered: {
    slug: string;
  };
  fd_select_viewed: {
    roles_shown: number;
  };
  fd_session_created: {
    cadence: "once" | "weekly";
    series_id: string;
  };
  fd_session_retired: {
    series_id: string;
  };
  fd_session_rsvp_withdrawn: {
    series_id: string;
  };
  fd_session_rsvped: {
    series_id: string;
  };
  fd_sessions_viewed: {
    upcoming: number;
  };
  fd_steward_claimed: {
    slug: string;
  };
  fd_steward_released: {
    slug: string;
  };
  fd_stewardship_viewed: {
    roles: number;
  };
  fd_timeline_viewed: {
    lane: "community" | "docs" | "exchange" | "harness" | "trajectory" | "worlds" | null;
  };
  fd_tour_completed: {
    steps: number;
  };
  fd_tour_dismissed: {
    step: number;
    steps: number;
  };
  fd_tour_started: {
    page: string;
    source: "auto" | "button";
    steps: number;
  };
  fd_tour_step_viewed: {
    page: string;
    step: number;
    step_id: string;
    steps: number;
  };
  fd_trajectory_inspected: {
    provenance: "bot" | "visitor";
    trajectory_id: string;
  };
  fd_transfer_accepted: {
    slug: string;
  };
  fd_transfer_offered: {
    slug: string;
  };
  fd_transfer_revoked: {
    slug: string;
  };
};
