export type GovernanceEvents = {
  gv_ban_name_description_submitted: {
    co_authors: number;
    length: number;
  };
  gv_ban_name_error: {
    error?: string;
  };
  gv_ban_name_name_invalid: {
    length: number;
  };
  gv_ban_name_name_submitted: {
    length: number;
  };
  gv_ban_name_review_reached: Record<string, never>;
  gv_ban_name_started: Record<string, never>;
  gv_ban_name_submitted: {
    proposal_id?: string;
    stub: true;
  };
  gv_ban_name_submitting: Record<string, never>;
  gv_bid_funding_set: {
    budget: number;
    duration: number;
    tender_id: string;
  };
  gv_bid_started: {
    tender_id: string;
  };
  gv_bid_step_advanced: {
    tender_id: string;
    to: string;
  };
  gv_bid_submit_attempted: {
    budget: number;
    duration: number;
    tender_id: string;
  };
  gv_bid_submitted: {
    budget: number;
    proposal_id?: string;
    published: boolean;
    tender_id: string;
  };
  gv_bid_vote_cast_failed: {
    attempt: number;
    bid_id: string;
    choice?: string;
  };
  gv_bid_vote_cast_reached: {
    bid_id: string;
    choice?: string;
  };
  gv_bid_vote_choice_selected: {
    bid_id: string;
    choice: string;
  };
  gv_bid_vote_completed: {
    bid_id: string;
    choice?: string;
    receipt?: string;
    stub: true;
  };
  gv_bid_vote_field_reviewed: {
    bid_id: string;
    bids: number;
  };
  gv_bid_vote_opened: {
    bid_id: string;
    bids: number;
    live: boolean;
    tender_id: string;
  };
  gv_bid_vote_snapshot_redirect: {
    attempts: number;
    bid_id: string;
  };
  gv_bid_vote_started: {
    bid_id: string;
    bids: number;
  };
  gv_catalyst_description_filled: {
    coauthors: number;
    length: number;
    request: "add" | "remove";
  };
  gv_catalyst_details_filled: {
    already_a_catalyst: boolean;
    request: "add" | "remove";
  };
  gv_catalyst_domain_invalid: {
    request: "add" | "remove";
  };
  gv_catalyst_review_reached: {
    request: "add" | "remove";
  };
  gv_catalyst_started: {
    request: "add" | "remove";
  };
  gv_catalyst_submit_error: {
    error?: string;
    request: "add" | "remove";
  };
  gv_catalyst_submitted: {
    proposal_id?: string;
    request: "add" | "remove";
  };
  gv_catalyst_submitting: {
    request: "add" | "remove";
  };
  gv_council_veto_coauthors_set: {
    coauthors: number;
  };
  gv_council_veto_reasons_filled: {
    has_suggestions: boolean;
    reasons_length: number;
  };
  gv_council_veto_review_reached: Record<string, never>;
  gv_council_veto_started: Record<string, never>;
  gv_council_veto_submit_error: {
    error?: string;
  };
  gv_council_veto_submitted: {
    proposal_id?: string;
  };
  gv_council_veto_submitting: Record<string, never>;
  gv_council_veto_url_invalid: Record<string, never>;
  gv_delegate_candidate_viewed: {
    candidate_id: string;
  };
  gv_delegate_completed: {
    candidate_id?: string;
    chain_id?: number;
    tx_hash?: string;
    tx_status?: "confirmed" | "pending";
    vp: null | number;
  };
  gv_delegate_confirm_reached: {
    candidate_id?: string;
    vp: null | number;
  };
  gv_delegate_signing: {
    candidate_id?: string;
    vp: null | number;
  };
  gv_delegate_started: {
    candidate_id: string;
  };
  gv_draft_coauthors_set: {
    count: number;
  };
  gv_draft_details_completed: {
    bodies: number;
    poll_id?: string;
    title_len: number;
  };
  gv_draft_started: {
    poll_id?: string;
  };
  gv_draft_step_advanced: {
    poll_id?: string;
    to: string;
  };
  gv_draft_submit_attempted: {
    coauthors: number;
    poll_id?: string;
    title_len: number;
  };
  gv_draft_submitted: {
    poll_id?: string;
    proposal_id?: string;
    simulated: true;
  };
  gv_govprop_details_invalid: {
    error_count: number;
  };
  gv_govprop_details_submitted: {
    bodies_filled: number;
    title_len: number;
  };
  gv_govprop_error: {
    error?: string;
  };
  gv_govprop_started: {
    linked_draft_id: string;
    vp: number;
  };
  gv_govprop_step_advanced: {
    co_authors: number;
    to: string;
  };
  gv_govprop_submit_attempted: {
    linked_draft_id: string;
    title_len: number;
  };
  gv_govprop_submitted: {
    proposal_id?: string;
  };
  gv_govprop_vp_blocked: {
    threshold: 2500;
    vp: number;
  };
  gv_grant_funding_set: {
    budget: number;
    category?: string;
    duration: number;
    tier?: string;
  };
  gv_grant_started: {
    category?: string;
  };
  gv_grant_step_advanced: {
    category?: string;
    to: string;
  };
  gv_grant_submit_attempted: {
    budget: number;
    category?: string;
    duration: number;
    tier?: string;
  };
  gv_grant_submitted: {
    budget: number;
    category?: string;
    proposal_id?: string;
    simulated: true;
  };
  gv_hiring_reasons_submitted: {
    co_authors: number;
    evidence_length: number;
    reasons_length: number;
    request: "add" | "remove";
  };
  gv_hiring_review_reached: {
    request: "add" | "remove";
  };
  gv_hiring_started: {
    request: "add" | "remove";
  };
  gv_hiring_submit_error: {
    error?: string;
    request: "add" | "remove";
  };
  gv_hiring_submitted: {
    proposal_id?: string;
    request: "add" | "remove";
  };
  gv_hiring_submitting: {
    request: "add" | "remove";
  };
  gv_hiring_target_invalid: {
    request: "add" | "remove";
  };
  gv_hiring_target_submitted: {
    committee: string;
    request: "add" | "remove";
  };
  gv_home_viewed: {
    ending_soon: number;
  };
  gv_link_connect_step: {
    account: "discord" | "forum" | "push";
    step: number;
  };
  gv_link_connected: {
    account: "discord" | "forum" | "push";
  };
  gv_link_started: {
    account: "discord" | "forum" | "push";
  };
  gv_link_unlinked: {
    account: "discord" | "forum" | "push";
  };
  gv_link_verify_error: {
    account: "discord" | "forum" | "push";
    error?: string;
  };
  gv_link_verifying: {
    account: "discord" | "forum" | "push";
  };
  gv_lw_collection_filled: {
    images: number;
    items: string;
  };
  gv_lw_identity_filled: {
    links: number;
  };
  gv_lw_review_reached: Record<string, never>;
  gv_lw_started: Record<string, never>;
  gv_lw_submit_error: {
    error?: string;
  };
  gv_lw_submitted: {
    proposal_id?: string;
  };
  gv_lw_submitting: Record<string, never>;
  gv_lw_technical_filled: {
    contracts: number;
    managers: number;
    programmatic: boolean;
  };
  gv_lw_validation_error: {
    fields: string[];
    step: string;
  };
  gv_pitch_coauthors_set: {
    count: number;
  };
  gv_pitch_details_invalid: {
    fields: string[];
  };
  gv_pitch_details_submitted: {
    body_chars: number;
    name_length: number;
  };
  gv_pitch_error: {
    error?: string;
  };
  gv_pitch_gate_passed: {
    vp: number;
  };
  gv_pitch_review_reached: Record<string, never>;
  gv_pitch_started: {
    meets_gate: boolean;
    vp: number;
  };
  gv_pitch_submitted: {
    proposal_id?: string;
    stub: true;
  };
  gv_pitch_submitting: Record<string, never>;
  gv_poi_coordinates_invalid: {
    request: "add" | "remove";
    x: string;
    y: string;
  };
  gv_poi_coordinates_submitted: {
    request: "add" | "remove";
    x: string;
    y: string;
  };
  gv_poi_description_submitted: {
    co_authors: number;
    length: number;
  };
  gv_poi_error: {
    error?: string;
    request: "add" | "remove";
  };
  gv_poi_review_reached: {
    request: "add" | "remove";
  };
  gv_poi_started: {
    request: "add" | "remove";
  };
  gv_poi_submitted: {
    proposal_id?: string;
    request: "add" | "remove";
    stub: true;
  };
  gv_poi_submitting: {
    request: "add" | "remove";
  };
  gv_profile_delegate_clicked: Record<string, never>;
  gv_profile_proposal_clicked: {
    proposal_id: string;
    tab: "coauthoring" | "proposals" | "watchlist";
  };
  gv_profile_tab_changed: {
    from_tab: "coauthoring" | "proposals" | "watchlist";
    to_tab: "coauthoring" | "proposals" | "watchlist";
  };
  gv_profile_viewed: {
    address: string;
    proposals_count: number;
    tab: "coauthoring" | "proposals" | "watchlist";
  };
  gv_project_clicked: {
    category: string;
    project_id: string;
    size: number;
    status: string;
    type: "bid" | "grant";
  };
  gv_project_tab_viewed: {
    project_id: string;
    tab: "activity" | "milestones" | "updates";
  };
  gv_project_vesting_clicked: {
    project_id: string;
    vesting_id: string;
  };
  gv_project_viewed: {
    project_id: string;
    source: "fallback" | "live";
    status: string;
  };
  gv_projects_filtered: {
    category: null | string;
    quarter: null | string;
    sort: null | string;
    status: null | string;
    subtype: null | string;
    year: null | string;
  };
  gv_projects_viewed: {
    bid_funding: number;
    category: null | string;
    count: number;
    grant_funding: number;
    quarter: null | string;
    sort: null | string;
    source: "live" | "unavailable";
    status: null | string;
    subtype: null | string;
    total: number;
    year: null | string;
  };
  gv_proposal_viewed: {
    category: string;
    proposal_id: string;
    status: string;
  };
  gv_proposals_clicked: Record<string, never>;
  gv_proposals_filtered: {
    category: null | string;
    search: null | string;
    status: null | string;
  };
  gv_proposals_viewed: {
    category: null | string;
    count: number;
    page: number;
    pageCount: number;
    search: null | string;
    status: null | string;
    total: number;
    totalFiltered: number;
  };
  gv_submit_category_selected: {
    category: string;
    group: null | string;
    route: string;
  };
  gv_submit_chooser_opened: {
    category: string;
    request: null | string;
  };
  gv_submit_group_filtered: {
    group: string;
  };
  gv_submit_hub_viewed: {
    categories: number;
    group: null | string;
  };
  gv_submit_poll_details_completed: {
    description_len: number;
    title_len: number;
  };
  gv_submit_poll_options_completed: {
    co_author_count: number;
    option_count: number;
  };
  gv_submit_poll_review_reached: {
    option_count: number;
  };
  gv_submit_poll_started: {
    connected: boolean;
    has_vp: boolean;
  };
  gv_submit_poll_submitted: {
    proposal_ref?: string;
    stub: true;
  };
  gv_submit_poll_vp_blocked: {
    connected: boolean;
    has_vp: boolean;
  };
  gv_tender_coauthors_set: {
    count: number;
  };
  gv_tender_details_filled: {
    linked_proposal_id: string;
    summary_len: number;
    target_release_quarter: string;
  };
  gv_tender_review_reached: {
    linked_proposal_id: string;
  };
  gv_tender_started: {
    linked_proposal_id: string;
    voting_power: number;
  };
  gv_tender_submit_error: {
    linked_proposal_id: string;
  };
  gv_tender_submitted: {
    linked_proposal_id: string;
    pending: boolean;
    proposal_id?: string;
  };
  gv_tender_submitting: {
    linked_proposal_id: string;
  };
  gv_tender_vp_gated: {
    threshold: number;
    voting_power: number;
  };
  gv_transparency_committee_expanded: {
    card: "expenses" | "income";
  };
  gv_transparency_dashboard_clicked: {
    href: string;
  };
  gv_transparency_viewed: {
    committees: number;
    source: "empty" | "error" | "live";
  };
  gv_update_comments_viewed: {
    project_id: string;
    total_comments: number;
    update_id: string;
  };
  gv_update_edit_confirm_open: {
    project_id: string;
    update_id: string;
  };
  gv_update_edit_financials: {
    project_id: string;
    records: number;
    update_id: string;
  };
  gv_update_edit_save_attempted: {
    project_id: string;
    update_id: string;
  };
  gv_update_edit_saved: {
    project_id: string;
    simulated: true;
    update_id: string;
  };
  gv_update_edit_started: {
    health: "atRisk" | "offTrack" | "onTrack";
    project_id: string;
    update_id: string;
  };
  gv_update_financials_set: {
    disclosed: number;
    project_id: string;
    records: number;
  };
  gv_update_notfound: {
    project_id: string;
  };
  gv_update_previewed: {
    health: string;
    project_id: string;
  };
  gv_update_publish_attempted: {
    disclosed: number;
    health: string;
    project_id: string;
  };
  gv_update_published: {
    health: string;
    project_id: string;
    simulated: true;
    update_id?: string;
  };
  gv_update_started: {
    health: string;
    project_id: string;
  };
  gv_update_viewed: {
    health: "atRisk" | "offTrack" | "onTrack" | null;
    project_id: string;
    source: "fixture" | "live";
    status: string;
    update_id: string;
    update_index: number;
  };
  gv_vote_completed: {
    choice: string;
    proposal_id: string;
    receipt?: string;
  };
  gv_vote_reasoned: {
    proposal_id: string;
  };
  gv_vote_snapshot_redirect: {
    attempts: number;
    proposal_id: string;
  };
  gv_vote_started: {
    choice: string;
    guided: boolean;
    proposal_id: string;
  };
};
