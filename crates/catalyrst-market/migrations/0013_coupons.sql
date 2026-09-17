-- Creator-signed discount coupons for the shop (port of marketplace-server
-- src/migrations/dapps/1789041600000_coupons.ts).
--
-- A coupon is an off-chain EIP-712 signature stored like a trade; the chain
-- only learns about it when a buyer applies it. `coupon_state` mirrors what the
-- CouponManager reports for each one (uses consumed, cancelled) plus whether
-- the signer's signature indexes have moved past the ones it was signed with,
-- refreshed by the coupon-state-refresh worker.
--
-- The upstream migration also GRANTs SELECT to a `mv_trades_owner` role. No
-- role split of that shape exists in this tree (the dapps read and write pools
-- reach the same role through search_path), so no grant is replayed here.

CREATE TABLE IF NOT EXISTS marketplace.coupons (
  id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  network           text NOT NULL,
  chain_id          integer NOT NULL,
  signer            varchar(42) NOT NULL,
  signature         text NOT NULL UNIQUE,
  hashed_signature  text NOT NULL UNIQUE,
  -- keccak256(abi.encode(signer, keccak256(signature))): the slot the
  -- CouponManager keys signatureUses and cancelledSignatures on.
  state_key         text NOT NULL UNIQUE,
  coupon_manager    varchar(42) NOT NULL,
  coupon_address    varchar(42) NOT NULL,
  checks            jsonb NOT NULL,
  discount_type     smallint NOT NULL,
  discount_ppm      integer NOT NULL,
  root              text NOT NULL,
  collections       text[] NOT NULL,
  effective_since   timestamptz(3) NOT NULL,
  expires_at        timestamptz(3) NOT NULL,
  created_at        timestamptz(3) NOT NULL DEFAULT now()::timestamptz(3),
  CONSTRAINT coupons_discount_ppm_within_bounds CHECK (discount_ppm BETWEEN 50000 AND 700000),
  CONSTRAINT coupons_effective_before_expiry CHECK (effective_since < expires_at)
);

-- Composite, in the order a creator's own list is served: filter by signer,
-- then newest first. A plain index on signer leaves the sort to be done by hand
-- on every page. No index on `collections`: nothing queries it directly.
CREATE INDEX IF NOT EXISTS idx_coupons_signer_created ON marketplace.coupons (signer, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_coupons_expires_at     ON marketplace.coupons (expires_at);

CREATE TABLE IF NOT EXISTS marketplace.coupon_state (
  coupon_id   uuid PRIMARY KEY REFERENCES marketplace.coupons(id) ON DELETE CASCADE,
  uses        integer NOT NULL DEFAULT 0,
  cancelled   boolean NOT NULL DEFAULT false,
  revoked     boolean NOT NULL DEFAULT false,
  checked_at  timestamptz(3) NOT NULL DEFAULT now()::timestamptz(3)
);

-- The refresh worker orders its batch by this column every tick.
CREATE INDEX IF NOT EXISTS idx_coupon_state_checked_at ON marketplace.coupon_state (checked_at);
