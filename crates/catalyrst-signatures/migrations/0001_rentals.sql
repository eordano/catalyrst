-- Port of decentraland/signatures-server migrations, consolidated:
--   1654545715004_rentals
--   1666283185308_add-target-column-to-rentals
--   1667818838202_add-rentedDays-and-periodChosen-columns
--   1676497054562_add-filters-to-listings
-- plus the implicit min_price/max_price columns referenced by the listings
-- sort (upstream computes them in the query; we materialize for sorting parity
-- via the aggregate, so no extra columns are needed there).

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

DO $$ BEGIN
  CREATE TYPE rental_status AS ENUM ('open', 'executed', 'cancelled', 'claimed');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN
  CREATE TYPE update_type AS ENUM ('metadata', 'rentals', 'indexes');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

CREATE TABLE IF NOT EXISTS metadata (
  id                TEXT PRIMARY KEY,
  category          TEXT NOT NULL,
  search_text       TEXT NOT NULL,
  -- filter columns (1676497054562_add-filters-to-listings)
  distance_to_plaza SMALLINT,
  adjacent_to_road  BOOLEAN,
  estate_size       SMALLINT,
  created_at        TIMESTAMP NOT NULL,
  updated_at        TIMESTAMP NOT NULL
);

CREATE TABLE IF NOT EXISTS rentals (
  id                       UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  metadata_id              TEXT NOT NULL REFERENCES metadata(id) ON DELETE CASCADE,
  network                  TEXT NOT NULL,
  chain_id                 INTEGER NOT NULL,
  contract_address         TEXT NOT NULL,
  token_id                 TEXT NOT NULL,
  expiration               TIMESTAMP NOT NULL,
  nonces                   TEXT[] NOT NULL,
  signature                TEXT NOT NULL,
  rental_contract_address  TEXT NOT NULL,
  status                   rental_status NOT NULL DEFAULT 'open',
  -- 1666283185308_add-target-column-to-rentals
  target                   TEXT NOT NULL DEFAULT '0x0000000000000000000000000000000000000000',
  -- 1667818838202_add-rentedDays-and-periodChosen-columns
  rented_days              INTEGER,
  period_chosen            UUID,
  created_at               TIMESTAMP NOT NULL DEFAULT now(),
  updated_at               TIMESTAMP NOT NULL DEFAULT now(),
  started_at               TIMESTAMP
);

CREATE TABLE IF NOT EXISTS rentals_listings (
  id      UUID PRIMARY KEY REFERENCES rentals(id) ON DELETE CASCADE,
  lessor  TEXT NOT NULL,
  tenant  TEXT
);

CREATE TABLE IF NOT EXISTS periods (
  id            UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  min_days      INTEGER NOT NULL CHECK (min_days >= 0),
  max_days      INTEGER NOT NULL CHECK (max_days >= min_days),
  price_per_day NUMERIC(78) NOT NULL CHECK (price_per_day >= 0),
  rental_id     UUID NOT NULL REFERENCES rentals_listings(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS updates (
  type       update_type PRIMARY KEY,
  updated_at TIMESTAMP NOT NULL DEFAULT to_timestamp(0)
);

CREATE INDEX IF NOT EXISTS periods_rental_id_index     ON periods (rental_id);
CREATE INDEX IF NOT EXISTS periods_min_days_index      ON periods (min_days);
CREATE INDEX IF NOT EXISTS periods_max_days_index      ON periods (max_days);
CREATE INDEX IF NOT EXISTS periods_price_per_day_index ON periods (price_per_day);
CREATE INDEX IF NOT EXISTS rentals_metadata_id_index   ON rentals (metadata_id);
CREATE INDEX IF NOT EXISTS rentals_signature_index     ON rentals (signature);
-- Ensure at most one OPEN rental per (token, contract). This is the constraint
-- the upstream create-handler relies on to surface RentalAlreadyExists (409).
CREATE UNIQUE INDEX IF NOT EXISTS rentals_token_id_contract_address_status_unique_index
  ON rentals (token_id, contract_address, status) WHERE status = 'open';
CREATE INDEX IF NOT EXISTS updates_updated_at_index ON updates (updated_at);

INSERT INTO updates (type, updated_at)
  VALUES ('metadata', now()), ('rentals', to_timestamp(0)), ('indexes', to_timestamp(0))
  ON CONFLICT (type) DO NOTHING;
