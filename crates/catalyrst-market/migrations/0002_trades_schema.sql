DO $$ BEGIN
  CREATE TYPE marketplace.trade_type AS ENUM ('bid', 'public_nft_order', 'public_item_order');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN
  CREATE TYPE marketplace.asset_direction_type AS ENUM ('sent', 'received');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

CREATE TABLE IF NOT EXISTS marketplace.trades (
  id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  network           text NOT NULL,
  chain_id          integer NOT NULL,
  signature         text NOT NULL UNIQUE,
  hashed_signature  text NOT NULL UNIQUE,
  checks            jsonb NOT NULL,
  signer            varchar(42) NOT NULL,
  type              marketplace.trade_type NOT NULL,
  expires_at        timestamptz(3) NOT NULL,
  effective_since   timestamptz(3) NOT NULL,
  contract          text NOT NULL DEFAULT '',
  created_at        timestamptz(3) NOT NULL DEFAULT now()::timestamptz(3)
);

CREATE INDEX IF NOT EXISTS idx_trades_signer_created ON marketplace.trades (signer, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_trades_created_at     ON marketplace.trades (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_trades_type           ON marketplace.trades (type);

CREATE TABLE IF NOT EXISTS marketplace.trade_assets (
  id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  trade_id          uuid NOT NULL REFERENCES marketplace.trades(id) ON DELETE CASCADE,
  direction         marketplace.asset_direction_type NOT NULL,
  asset_type        smallint NOT NULL,
  contract_address  varchar(42) NOT NULL,
  beneficiary       varchar(42),
  extra             text,
  created_at        timestamptz(3) NOT NULL DEFAULT now()::timestamptz(3)
);

CREATE INDEX IF NOT EXISTS idx_trade_assets_trade ON marketplace.trade_assets (trade_id);

CREATE TABLE IF NOT EXISTS marketplace.trade_assets_erc721 (
  asset_id  uuid NOT NULL UNIQUE REFERENCES marketplace.trade_assets(id) ON DELETE CASCADE,
  token_id  text NOT NULL
);

CREATE TABLE IF NOT EXISTS marketplace.trade_assets_erc20 (
  asset_id  uuid NOT NULL UNIQUE REFERENCES marketplace.trade_assets(id) ON DELETE CASCADE,
  amount    numeric(78,0) NOT NULL CHECK (amount >= 0)
);

CREATE TABLE IF NOT EXISTS marketplace.trade_assets_item (
  asset_id  uuid NOT NULL UNIQUE REFERENCES marketplace.trade_assets(id) ON DELETE CASCADE,
  item_id   text NOT NULL
);
