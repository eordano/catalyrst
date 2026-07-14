export type CreatorHubEvents = {
  bd_create_collection_completed: {
    collection_id?: string;
    contract_address?: string;
    count: number;
    stub: true;
    type: "linked" | "standard";
  };
  bd_create_collection_items_added: {
    count: number;
  };
  bd_create_collection_named: {
    name: string;
  };
  bd_create_collection_review_reached: {
    cost_mana: number;
    count: number;
    type: "linked" | "standard";
  };
  bd_create_collection_started: {
    type: "linked" | "standard";
  };
  bd_create_collection_submitted: {
    cost_mana: number;
    count: number;
    type: "linked" | "standard";
  };
  bd_curation_assigned: {
    id: string;
  };
  bd_curation_comment_added: {
    decision?: "approved" | "rejected";
    has_comment: true;
    id?: string;
    length: number;
    stub: true;
    topic_id: null | number;
  };
  bd_curation_decided: {
    has_comment: boolean;
    id?: string;
    status?: "approved" | "rejected";
    stub: true;
    updated?: number;
  };
  bd_curation_filtered: {
    assignee: string;
    status: "ALL_STATUS" | "approved" | "rejected" | "to_review" | "under_review";
    type: "ALL_TYPES" | "standard" | "third_party";
  };
  bd_curation_review_opened: {
    id: string;
  };
  bd_curation_viewed: {
    count: number;
  };
  bd_item_category_set: {
    category: string;
    item: string;
  };
  bd_item_editor_opened: {
    collection: string;
    item: string;
  };
  bd_item_model_set: {
    item: string;
    model: string;
  };
  bd_item_price_set: {
    free: boolean;
    item: string;
    price: string;
  };
  bd_item_rarity_set: {
    item: string;
    max_supply: number;
    rarity: string;
  };
  bd_item_reverted: {
    item: string;
  };
  bd_item_saved: {
    item: string;
    price: string;
    rarity: string;
    stub: true;
    urn?: string;
  };
  bd_publish_collection_cost_shown: {
    mana: number;
  };
  bd_publish_collection_started: {
    id: string;
    itemCount: number;
  };
  bd_publish_collection_terms_accepted: Record<string, never>;
  bd_publish_fee_paid: {
    mana: number;
    simulated: true;
    tx_hash?: string;
  };
  bd_publish_submitted: {
    id: string;
    itemCount: number;
    mana: number;
    stub: true;
  };
  ch_claim_name_available: {
    name: string;
  };
  ch_claim_name_completed: {
    name: string;
    stub: true;
    token_id?: string;
    tx_hash?: string;
    world_name: string;
  };
  ch_claim_name_mint_submitted: {
    name: string;
    simulated: true;
  };
  ch_claim_name_returned_to_publish: {
    name: string;
    simulated?: true;
    world_name?: string;
  };
  ch_claim_name_review_reached: {
    name: string;
    price_mana: string;
  };
  ch_claim_name_started: {
    name: string;
  };
  ch_claim_name_unavailable: {
    name: string;
  };
  ch_create_project_completed: {
    files: string[];
    folder?: string;
    name: string;
    path: string;
    template: string;
    via?: "canceled" | "directory" | "download";
    written: boolean;
  };
  ch_create_project_failed: {
    code?: string;
    message: string;
  };
  ch_create_project_name_set: {
    name: string;
  };
  ch_create_project_path_invalid: {
    path: string;
  };
  ch_create_project_path_set: {
    path: string;
  };
  ch_create_project_scaffolding: {
    name: string;
    path: string;
    template: string;
  };
  ch_create_project_started: Record<string, never>;
  ch_create_project_template_selected: {
    preselected?: true;
    template: string;
  };
  ch_curate_signin_clicked: Record<string, never>;
  ch_delete_cancelled: {
    local?: true;
    project_id: string;
  };
  ch_delete_confirmed: {
    delete_files: boolean;
    local?: true;
    local_files?: "deleted" | "kept";
    overrode_pointers?: number;
    project_id: string;
    replaced_entity?: string;
    simulated: false;
    status?: number;
    tombstone_id?: string;
  };
  ch_delete_done_viewed: {
    deleted_id: null | string;
    remaining: number;
  };
  ch_delete_failed: {
    error: string;
    project_id: string;
    simulated: false;
    status: number;
  };
  ch_delete_files_opted_in: {
    local?: true;
    project_id: string;
  };
  ch_delete_files_toggled: {
    checked: boolean;
    local?: true;
    project_id: string;
  };
  ch_delete_no_deployment: {
    base: string;
    project_id: string;
  };
  ch_delete_opened: {
    local?: true;
    project_id: string;
    published: boolean | null;
  };
  ch_delete_scenes_viewed: {
    count: number;
    source: "empty" | "live" | "unavailable";
  };
  ch_deploy_world_completed: {
    jump_url?: string;
    name?: string;
    target: "land" | "world";
  };
  ch_deploy_world_confirm_reached: {
    name?: string;
    target: "land" | "world";
  };
  ch_deploy_world_destination_selected: {
    target: "land" | "world";
  };
  ch_deploy_world_failed: {
    error?: string;
    name?: string;
  };
  ch_deploy_world_name_selected: {
    name?: string;
  };
  ch_deploy_world_names_empty: Record<string, never>;
  ch_deploy_world_quota_exceeded: {
    max_mb: number;
    phase?: string;
    total_bytes: number;
  };
  ch_deploy_world_review_reached: {
    exceeded: boolean;
    target: "land" | "world";
    total_bytes: number;
  };
  ch_deploy_world_started: {
    target: "land" | "world";
  };
  ch_editor_asset_searched: {
    query: string;
  };
  ch_editor_assets_browsed: Record<string, never>;
  ch_editor_component_added: {
    component: string;
    on?: number;
  };
  ch_editor_entity_created: {
    asset_id?: string;
    asset_name?: string;
    entity?: number;
  };
  ch_editor_entity_deleted: {
    entity?: number;
    name?: string;
  };
  ch_editor_entity_modified: {
    entity?: number;
    name?: string;
  };
  ch_editor_entity_renamed: {
    entity?: number;
    name: string;
  };
  ch_editor_opened: Record<string, never>;
  ch_editor_saved: {
    component?: string;
    composite?: string;
    deleted: boolean;
    entities?: number;
    entity?: number;
    mode: string;
    renamed?: string;
    stub: boolean;
    via?: "canceled" | "download" | "fsa-handle";
  };
  ch_editor_transform_set: {
    axis: string;
  };
  ch_home_card_opened: {
    card: string;
  };
  ch_home_signin_clicked: Record<string, never>;
  ch_home_start_building: Record<string, never>;
  ch_home_viewed: {
    scene_count: number;
  };
  ch_learn_signin_clicked: Record<string, never>;
  ch_learn_viewed: Record<string, never>;
  ch_manage_card_clicked: {
    id: string;
    role: "collaborator" | "operator" | "owner";
  };
  ch_manage_empty_viewed: {
    filter: "published" | "unpublished";
  };
  ch_manage_filter_changed: {
    filter: "published" | "unpublished";
  };
  ch_manage_searched: {
    q: null | string;
  };
  ch_manage_sorted: {
    sort: "domain" | "last_published";
  };
  ch_manage_storage_opened: Record<string, never>;
  ch_manage_viewed: {
    address: null | string;
    count: number;
    filter: "published" | "unpublished";
    search: null | string;
    sort: "domain" | "last_published";
  };
  ch_mp_paired: Record<string, never>;
  ch_mp_replay_completed: {
    run: null | string;
    tier: "a" | "b";
  };
  ch_mp_replay_failed: {
    error: string;
    run: null | string;
  };
  ch_mp_replay_requested: {
    run: null | string;
    tier?: "a" | "b";
  };
  ch_mp_run_completed: {
    run: null | string;
  };
  ch_mp_run_failed: {
    detail?: string;
    run: null | string;
  };
  ch_mp_run_launched: Record<string, never>;
  ch_mp_run_rejected: {
    error: string;
  };
  ch_scenes_clicked: {
    card?: string;
    entity?: string;
    from?: string;
    local?: true;
    ok?: boolean;
    parcel?: string;
    seeded?: boolean;
    to?: string;
  };
  ch_scenes_empty_viewed: Record<string, never>;
  ch_scenes_import_failed: {
    files?: number;
    has_scene_json?: boolean;
    reason: string;
  };
  ch_scenes_signin_clicked: Record<string, never>;
  ch_scenes_viewed: {
    count: number;
    empty: boolean;
    error: boolean;
  };
  ch_studio_opened: {
    source: string;
    template_id: string;
  };
  ch_template_previewed: {
    template_id: string;
  };
  ch_template_selected: {
    template_id: string;
    title: string;
  };
  ch_template_view_code: {
    template_id: string;
  };
  ch_templates_signin_clicked: Record<string, never>;
  ch_templates_viewed: Record<string, never>;
  ch_world_perms_access_type_set: {
    access_type: string;
  };
  ch_world_perms_collaborator_validated: {
    address?: string;
    valid: boolean;
  };
  ch_world_perms_completed: {
    access_type: "allow-list" | "shared-secret" | "unrestricted";
    addresses?: number;
    stub: boolean;
  };
  ch_world_perms_confirm_reached: {
    access_type: "allow-list" | "shared-secret" | "unrestricted";
    collaborators: number;
  };
  ch_world_perms_invalid_address: Record<string, never>;
  ch_world_perms_invite_submitted: {
    channel: "community" | "csv" | "wallet";
  };
  ch_world_perms_password_set: Record<string, never>;
  ch_world_perms_started: Record<string, never>;
  ch_world_settings_changed: {
    field: string;
    tab: "details" | "layout" | "misc";
    world: string;
  };
  ch_world_settings_discarded: {
    change_count: number;
    world: string;
  };
  ch_world_settings_opened: {
    world: string;
  };
  ch_world_settings_review_reached: {
    change_count: number;
    world: string;
  };
  ch_world_settings_saved: {
    fields: string[];
    stub: boolean;
    world: string;
  };
  ch_world_settings_saving: {
    fields: string[];
    world: string;
  };
  ch_world_settings_tab_viewed: {
    tab: "details" | "layout" | "misc";
    world: string;
  };
  ch_worlds_storage_asset_selected: {
    kind: string;
    world: string;
  };
  ch_worlds_storage_clear_failed: {
    count: number;
    error: string;
    namespace: "env" | "players" | "scene";
    world: null | string;
  };
  ch_worlds_storage_cleared: {
    count: number;
    namespace: "env" | "players" | "scene";
    reason?: string;
    stub?: true;
    world: null | string;
  };
  ch_worlds_storage_dialog_opened: {
    key: null | string;
    mode: string;
    world: null | string;
  };
  ch_worlds_storage_namespace_changed: {
    namespace: "env" | "players" | "scene";
    world: null | string;
  };
  ch_worlds_storage_quota_opened: {
    world: null | string;
  };
  ch_worlds_storage_quota_retry: {
    world: null | string;
  };
  ch_worlds_storage_value_delete_failed: {
    error: string;
    key: string;
    namespace: "env" | "players" | "scene";
    world: null | string;
  };
  ch_worlds_storage_value_deleted: {
    key: string;
    namespace: "env" | "players" | "scene";
    reason?: string;
    stub?: true;
    world: null | string;
  };
  ch_worlds_storage_value_save_failed: {
    error: string;
    key: null | string;
    mode: string;
    namespace: "env" | "players" | "scene";
    world: null | string;
  };
  ch_worlds_storage_value_saved: {
    key: null | string;
    mode: string;
    namespace: "env" | "players" | "scene";
    reason?: string;
    stub?: true;
    world: null | string;
  };
  ch_worlds_storage_viewed: {
    fallback: boolean;
    lands: number;
    namespace: "env" | "players" | "scene";
    source: "empty" | "live";
    step: "clear" | "edit" | "scene" | "select";
    world: null | string;
    worlds: number;
  };
  create_capability_detected: {
    fs_access: boolean;
    route: string;
  };
  create_download_clicked: {
    from: string;
  };
  create_entry_viewed: {
    arm: string;
  };
  create_preview: {
    path: string;
    target: "/create/wearables/item-editor?from=create-entry" | "/creator-hub/scene-editor?new=1&from=create-entry";
  };
  creator_builder_redirect: {
    from: string;
    fromPath?: string;
    query?: string;
    to: string;
  };
  creator_builder_redirect_dashboard_viewed: {
    total: number;
    window_days: number;
  };
  creator_claim_name_redirect: {
    from: string;
    to: string;
  };
  creator_collection_detail_viewed: {
    id: string;
    itemCount: number;
    missing: boolean;
    source: "catalyst" | "empty";
    tab: "emotes" | "wearables";
  };
  creator_collection_item_clicked: {
    id: string;
    itemId: string;
    kind: string;
    tab: "emotes" | "wearables";
  };
  creator_collection_published: {
    funnel_step: number;
    source_event?: string;
  };
  creator_collection_tab_changed: {
    tab: "emotes" | "wearables";
  };
  creator_collection_viewed: {
    contract?: string;
    funnel_step: number;
    item_count?: number;
    source_event?: string;
  };
  creator_curation_submitted: {
    funnel_step: number;
    source_event?: string;
  };
  creator_dashboard_viewed: {
    on_sale_items: null | number;
    published_collections: null | number;
    sales_7d: null | number;
    scene_visits_30d: null | number;
    window_days: number;
  };
  creator_item_detail_viewed: {
    fallback: boolean;
    id: string;
    type: string;
  };
  creator_item_edit_clicked: {
    id: string;
  };
  creator_item_edited: {
    funnel_step: number;
    source_event?: string;
  };
  creator_item_listed: {
    funnel_step: number;
    source_event?: string;
  };
  creator_publish_started: {
    funnel_step: number;
    source_event?: string;
  };
  creator_sale_completed: {
    funnel_step: number;
    source_event?: string;
  };
  creator_store_viewed: {
    contract?: string;
    floor?: null | string;
    funnel_step: number;
    source_event?: string;
  };
  creator_wearables_card_clicked: {
    id: string;
    kind: string;
  };
  creator_wearables_home_viewed: {
    count: number;
  };
  creator_wearables_signin_clicked: Record<string, never>;
  creator_wearables_view_changed: {
    view: "grid" | "list";
  };
};
