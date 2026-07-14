-- Reconcile the community role CHECK to the canonical set: drop the never-used
-- legacy 'admin' value and allow 'none' (which apply_leave / apply_unban insert).
--
-- This change was originally edited into 0002_federation.sql in place, which broke
-- the sqlx migration checksum on already-migrated deployments ("migration 2 was
-- previously applied but has been modified"). 0002 is restored to its original
-- bytes and the change lives here as an additive migration instead.
ALTER TABLE community_role_log
    DROP CONSTRAINT IF EXISTS community_role_log_role_check,
    ADD  CONSTRAINT community_role_log_role_check
         CHECK (role IN ('owner','mod','member','banned','none'));

ALTER TABLE community_role_current
    DROP CONSTRAINT IF EXISTS community_role_current_role_check,
    ADD  CONSTRAINT community_role_current_role_check
         CHECK (role IN ('owner','mod','member','banned','none'));
