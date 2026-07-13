-- Baseline presence is separate from worn urns: an address whose first-seen
-- profile wears no marketplace items still has a (empty) baseline — without
-- this row, its genuine first wear would be swallowed by the seed path.
-- (Own migration: the first-wear DDL was already applied before the gate
-- bug was found; applied migrations are immutable.)
CREATE TABLE IF NOT EXISTS first_wear_baseline (
    address   TEXT PRIMARY KEY,
    seeded_at BIGINT NOT NULL
);
