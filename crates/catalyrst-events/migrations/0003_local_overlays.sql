-- Local write overlays the sqlx migrator must own alongside the 0001
-- federation tables — the queries in src/ports assume both exist on every
-- node, and an authenticated GET (which reads event_attendance_local for the
-- viewer's `attending` flag) is a 500 on a node without them while the same
-- request unsigned serves fine. IF NOT EXISTS keeps a node that already
-- carries externally-created copies a no-op, same as the 0002 `event` table.
--
-- event_attendance_local: RSVP state for /api/events/{id}/attendees, written
-- by signed-fetch going/cancel actions; read by every authenticated event
-- list (local_attending_set in src/ports/events.rs) and by attendee listings
-- (src/ports/attendees.rs).

CREATE TABLE IF NOT EXISTS event_attendance_local (
  event_id        TEXT NOT NULL,
  signer          TEXT NOT NULL,
  signed_payload  JSONB NOT NULL DEFAULT '{}'::jsonb,
  action          TEXT NOT NULL,
  signed_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (event_id, signer)
);

-- local_attending_set filters by (signer, action) with no event_id bound.
CREATE INDEX IF NOT EXISTS event_attendance_local_signer_action_idx
  ON event_attendance_local (signer, action);

-- events_local: signed event-create/edit payload overlay merged over the
-- mirrored `event` row (upsert_local / get_local in src/ports/events.rs).
CREATE TABLE IF NOT EXISTS events_local (
  id              TEXT PRIMARY KEY,
  signer          TEXT NOT NULL,
  signed_payload  JSONB NOT NULL DEFAULT '{}'::jsonb,
  signed_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
