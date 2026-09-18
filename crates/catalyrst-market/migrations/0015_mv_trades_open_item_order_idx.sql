-- The items feeds look up one open public_item_order per page item; cover that lookup directly.
CREATE INDEX IF NOT EXISTS idx_mv_trades_open_item_order
    ON marketplace.mv_trades (sent_contract_address, sent_item_id)
    WHERE status = 'open' AND type = 'public_item_order';
