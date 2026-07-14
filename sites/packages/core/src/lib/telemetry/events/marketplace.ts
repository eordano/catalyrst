export type MarketplaceEvents = {
  cart_remove: {
    collection: string;
    item_id: string;
  };
  cart_viewed: {
    item_count: number;
    total_credits: string;
  };
  mk_accept_bid_completed: {
    bid_id: string;
    stub: true;
    tx_hash?: string;
  };
  mk_accept_bid_confirm_reached: {
    bid_id: string;
    price: string;
  };
  mk_accept_bid_nft_approved: {
    bid_id: string;
    simulated: true;
  };
  mk_accept_bid_rejected: {
    bid_id: string;
  };
  mk_accept_bid_started: {
    bid_id: string;
    price: string;
  };
  mk_accept_bid_submitted: {
    bid_id: string;
  };
  mk_accept_bid_wallet_connected: {
    bid_id: string;
  };
  mk_account_asset_clicked: {
    item_id: string;
  };
  mk_account_tab_changed: {
    tab: "bids" | "collections" | "on-rent" | "on-sale" | "overview" | "social";
  };
  mk_account_viewed: {
    names: number;
    on_rent: number;
    on_sale: number;
    owned: number;
    source: "empty" | "live";
  };
  mk_activity_filter_applied: {
    type: "bid" | "listing" | "sale" | null;
  };
  mk_activity_paginated: {
    direction: "next" | "prev";
    page: number;
  };
  mk_activity_row_clicked: {
    id: string;
    kind: "bid" | "listing" | "sale";
  };
  mk_activity_viewed: {
    count: number;
    page: number;
    type: "bid" | "listing" | "sale" | null;
  };
  mk_add_to_cart: {
    item_id: string;
  };
  mk_asset_viewed: {
    item_id: string;
    kind: "collectible" | "land" | null;
    on_sale: boolean;
    rarity: null | string;
  };
  mk_bid_amount_set: {
    price: number;
  };
  mk_bid_completed: {
    expiration?: string;
    price?: number;
    stub: true;
  };
  mk_bid_confirmed: {
    price?: number;
  };
  mk_bid_expiration_set: {
    expiration: string;
  };
  mk_bid_failed: {
    where?: string;
  };
  mk_bid_insufficient_mana: {
    balance: number;
    price: number;
  };
  mk_bid_mana_approved: {
    price?: number;
  };
  mk_bid_sign_reached: {
    price?: number;
  };
  mk_bid_signed: {
    price?: number;
  };
  mk_bid_started: Record<string, never>;
  mk_buy_clicked: {
    item_id: string;
  };
  mk_buy_completed: {
    asset_id: string;
    stub: true;
    tx_hash?: string;
  };
  mk_buy_confirm_reached: {
    asset_id: string;
    price_wei: string;
  };
  mk_buy_failed: {
    asset_id: string;
    error?: string;
  };
  mk_buy_mana_approved: {
    asset_id: string;
    price_mana: string;
  };
  mk_buy_started: {
    asset_id: string;
    price_mana: string;
  };
  mk_buy_viewed: {
    asset_id: string;
    network: "ethereum" | "polygon";
    price_mana: string;
    source: "catalyst" | "empty" | "unavailable";
  };
  mk_buy_wallet_connected: {
    asset_id: string;
  };
  mk_cancel_completed: {
    order_id?: string;
    order_signature_hash?: string;
    stub: true;
  };
  mk_cancel_confirm_reached: {
    order_id?: string;
    price?: string;
  };
  mk_cancel_failed: {
    error?: string;
    order_id?: string;
  };
  mk_cancel_listing_viewed: {
    has_listing: boolean;
    order_id: null | string;
    ownership: "none" | "other" | "self";
    price: null | string;
    source: "catalyst" | "empty" | "unavailable";
  };
  mk_cancel_not_owner: {
    order_id?: string;
    ownership: "none" | "other" | "self";
  };
  mk_cancel_started: {
    order_id?: string;
  };
  mk_cancel_submitted: {
    order_id?: string;
  };
  mk_cancel_wallet_connected: {
    order_id?: string;
  };
  mk_card_topup: {
    credits: string;
    mock: true;
  };
  mk_checkout_confirm_reached: {
    total_credits: string;
  };
  mk_checkout_failed: {
    error?: string;
    status?: string;
    total_credits: string;
  };
  mk_checkout_processing: {
    checkout_id?: number;
    status?: string;
    total_credits: string;
  };
  mk_checkout_started: {
    total_credits: string;
  };
  mk_checkout_succeeded: {
    checkout_id?: number;
    status?: string;
    total_credits: string;
  };
  mk_claim_name_available: {
    name: string;
  };
  mk_claim_name_completed: {
    name: string;
    stub: true;
    token_id?: string;
    tx_hash?: string;
  };
  mk_claim_name_confirm_reached: {
    name: string;
    price_mana: string;
  };
  mk_claim_name_mana_approved: {
    name: string;
    price_mana: string;
    simulated: true;
  };
  mk_claim_name_started: {
    name: string;
  };
  mk_claim_name_submitted: {
    name: string;
    simulated: true;
  };
  mk_claim_name_unavailable: {
    name: string;
  };
  mk_claim_signin_gated: {
    name: string;
  };
  mk_collection_empty: {
    contract: string;
    fallback: boolean;
  };
  mk_collection_item_clicked: {
    contract: string;
    item_id: string;
  };
  mk_collection_sorted: {
    contract: string;
    sort_by: string;
  };
  mk_collection_viewed: {
    contract: string;
    floor: null | string;
    item_count: number;
    sort_by: string;
  };
  mk_credits_balance_viewed: {
    available: number;
    blocked: boolean;
    claimable: number;
    expires_in_seconds: number;
  };
  mk_credits_claim_clicked: {
    goal: string;
    reward: number;
  };
  mk_credits_goal_viewed: {
    claimable: number;
    completed: number;
    count: number;
  };
  mk_credits_viewed: {
    goal_count: number;
    has_started: boolean;
    week: number;
  };
  mk_favorite_sync_failed: {
    item_id: string;
    on: boolean;
  };
  mk_favorite_sync_unavailable: {
    item_id: string;
    on: boolean;
  };
  mk_favorite_toggle: {
    item_id: string;
    signed_in: boolean;
  };
  mk_list_item_clicked: {
    item_id: string;
    list_id: string;
  };
  mk_list_opened: {
    items_count: number;
    list_id: string;
  };
  mk_lists_viewed: {
    count: number;
  };
  mk_make_offer_clicked: {
    item_id: string;
  };
  mk_mana_topup_granted: {
    credits: string;
    tx: string;
  };
  mk_mana_topup_relayed: {
    tx: string;
  };
  mk_manage_action_clicked: {
    action: "cancel" | "sell" | "transfer";
    item_id: string;
  };
  mk_manage_asset_viewed: {
    has_rental: boolean;
    item_id: null | string;
    listed: boolean;
    network: "ethereum" | "polygon" | null;
  };
  mk_mint_completed: {
    item_id: string;
    stub: true;
    trade_id: null | string;
    tx_hash?: string;
  };
  mk_mint_confirm_reached: {
    item_id: string;
    price_mana: null | string;
  };
  mk_mint_failed: {
    error?: string;
    item_id: string;
    step?: "approve" | "connect" | "submit";
  };
  mk_mint_mana_approved: {
    item_id: string;
  };
  mk_mint_review_confirmed: {
    item_id: string;
    price_mana: null | string;
  };
  mk_mint_started: {
    item_id: string;
  };
  mk_mint_submitted: {
    item_id: string;
    trade_id: null | string;
  };
  mk_mint_wallet_connected: {
    item_id: string;
  };
  mk_names_buy_clicked: {
    name: string;
    price_wei: string;
  };
  mk_names_checked: {
    query: string;
    result: string;
  };
  mk_names_claim_clicked: {
    name: string;
  };
  mk_names_signin_gated: {
    action: string;
    query: string;
  };
  mk_names_tab: {
    tab: string;
  };
  mk_pay_method_selected: {
    method: "card" | "mana";
  };
  mk_rent_abandoned: Record<string, never>;
  mk_rent_completed: {
    days?: number;
    rental_id?: string;
    stub: true;
    total_mana?: number;
    tx_hash?: string;
  };
  mk_rent_failed: {
    error?: string;
  };
  mk_rent_mana_approved: Record<string, never>;
  mk_rent_period_selected: {
    max_days?: number;
    min_days?: number;
    period_index?: number;
  };
  mk_rent_price_set: {
    days?: number;
    total_mana?: number;
  };
  mk_rent_retried: Record<string, never>;
  mk_rent_sign_reached: Record<string, never>;
  mk_rent_signed: Record<string, never>;
  mk_rent_started: Record<string, never>;
  mk_sell_approve_reached: {
    item_id?: string;
  };
  mk_sell_asset_selected: {
    item_id: string;
  };
  mk_sell_completed: {
    approval_tx_hash: null | string;
    item_id?: string;
    order_id?: string;
    price_mana?: number;
  };
  mk_sell_confirm_reached: {
    item_id?: string;
    price_mana?: number;
  };
  mk_sell_expiration_set: {
    expires_at: number;
  };
  mk_sell_failed: {
    item_id?: string;
    reason?: string;
  };
  mk_sell_price_invalid: {
    price_mana?: number;
  };
  mk_sell_price_set: {
    price_mana: number;
  };
  mk_sell_sign_reached: {
    item_id?: string;
  };
  mk_sell_started: Record<string, never>;
  mk_settings_sign_out: Record<string, never>;
  mk_settings_tab_changed: {
    tab: "authorizations" | "store";
  };
  mk_settings_viewed: {
    granted: number;
    source: "catalyst" | "empty" | "unavailable";
    tab: "authorizations" | "store";
  };
  mk_shop_buy_now: {
    item_id: string;
  };
  mk_shop_card_clicked: {
    item_id: string;
  };
  mk_shop_clear_filters: Record<string, never>;
  mk_shop_filter: {
    filter: string;
    value: null | string;
  };
  mk_shop_page: {
    page: number;
  };
  mk_shop_search: {
    has_query: boolean;
  };
  mk_shop_sort: {
    sort_by: string;
  };
  mk_shop_tab: {
    tab: string;
  };
  mk_store_field_edited: {
    field: string;
  };
  mk_store_save_clicked: {
    deferred: true;
  };
  mk_transfer_asset_selected: {
    item_id: string;
  };
  mk_transfer_completed: {
    item_id?: string;
    recipient: string;
    stub: true;
    tx_hash?: string;
  };
  mk_transfer_confirm_reached: {
    item_id?: string;
  };
  mk_transfer_invalid_recipient: Record<string, never>;
  mk_transfer_recipient_entered: {
    recipient: string;
  };
  mk_transfer_reviewed: {
    item_id?: string;
    recipient: string;
  };
  mk_transfer_started: {
    item_id: string;
  };
  mk_transfer_submitted: {
    item_id?: string;
    recipient: string;
    stub: true;
  };
  mk_view_listing_clicked: {
    item_id: string;
    token_id: string;
  };
  pack_purchase_started: {
    credits: string;
    price_cents: number;
    sku: string;
  };
  pack_purchased: {
    credits: null | string;
    payment_intent: string;
    sku: null | string;
  };
  pack_viewed: {
    count: number;
    skus: string[];
    source: "empty" | "live" | "unavailable";
  };
};
