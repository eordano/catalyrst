-- Per-issue workflow state (resolve / ignore / unresolve), keyed by the same
-- fingerprint the issues view groups on. The dashboard LEFT JOINs this; an issue
-- with no row is implicitly 'unresolved'. A 'resolved' issue that receives a
-- newer event than its resolution time is treated as regressed (-> unresolved)
-- by the read query, matching Sentry's auto-reopen.
CREATE TABLE IF NOT EXISTS issue_state (
    fingerprint text PRIMARY KEY,
    status      text NOT NULL DEFAULT 'unresolved'
                CHECK (status IN ('unresolved','resolved','ignored')),
    assignee    text,
    note        text,
    updated_at  timestamptz NOT NULL DEFAULT now()
);
