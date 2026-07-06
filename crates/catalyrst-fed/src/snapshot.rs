use crate::session::Scope;
use serde::{Deserialize, Serialize};

pub fn path_snapshot(scope: Scope) -> String {
    format!("/federation/{}/snapshot", scope.as_str())
}

pub fn path_changes(scope: Scope) -> String {
    format!("/federation/{}/changes", scope.as_str())
}

pub type LogWatermark = i64;

pub type Cursor = i64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub seq: i64,
    pub signature_hash: String,
}

pub fn caught_up(cursor: Cursor, watermark: LogWatermark) -> bool {
    cursor >= watermark
}

pub fn next_cursor(prev: Cursor, applied: &[Change]) -> Cursor {
    applied
        .iter()
        .map(|c| c.seq)
        .max()
        .unwrap_or(prev)
        .max(prev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_scope_namespaced() {
        assert_eq!(path_snapshot(Scope::Places), "/federation/places/snapshot");
        assert_eq!(path_changes(Scope::Places), "/federation/places/changes");
        assert_eq!(
            path_snapshot(Scope::Communities),
            "/federation/communities/snapshot"
        );
        assert_eq!(
            path_changes(Scope::Messaging),
            "/federation/messaging/changes"
        );
    }

    #[test]
    fn cursor_advances_to_last_seq_and_never_regresses() {
        assert_eq!(next_cursor(0, &[]), 0, "empty page keeps cursor put");
        let page = vec![
            Change {
                seq: 3,
                signature_hash: "a".into(),
            },
            Change {
                seq: 7,
                signature_hash: "b".into(),
            },
        ];
        assert_eq!(next_cursor(0, &page), 7, "advance to max seq in page");

        let stale = vec![Change {
            seq: 2,
            signature_hash: "c".into(),
        }];
        assert_eq!(next_cursor(7, &stale), 7);
    }

    #[test]
    fn caught_up_when_cursor_reaches_watermark() {
        assert!(!caught_up(0, 5));
        assert!(!caught_up(4, 5));
        assert!(caught_up(5, 5));
        assert!(
            caught_up(6, 5),
            "cursor past watermark (new writes) is fine"
        );
        assert!(caught_up(0, 0), "empty log is trivially caught up");
    }

    #[test]
    fn reconciliation_loop_pages_to_watermark_and_dedups() {
        let log: Vec<Change> = (1..=10)
            .map(|seq| Change {
                seq,
                signature_hash: format!("sig{seq}"),
            })
            .collect();
        let watermark: LogWatermark = log.last().unwrap().seq;

        let page = |since: Cursor, limit: usize| -> Vec<Change> {
            log.iter()
                .filter(|c| c.seq > since)
                .take(limit)
                .cloned()
                .collect()
        };

        let mut applied: std::collections::HashSet<String> = Default::default();
        let mut apply_count = 0usize;
        let mut cursor: Cursor = 0;
        let mut pages = 0;
        while !caught_up(cursor, watermark) {
            let rows = page(cursor, 4);
            assert!(!rows.is_empty(), "non-caught-up cursor must yield rows");
            for c in &rows {
                if applied.insert(c.signature_hash.clone()) {
                    apply_count += 1;
                }
            }
            cursor = next_cursor(cursor, &rows);
            pages += 1;
            assert!(pages < 100, "loop must terminate");
        }
        assert_eq!(apply_count, 10, "every row applied exactly once");
        assert_eq!(cursor, watermark);

        let replay_rows = page(3, 100);
        let before = apply_count;
        for c in &replay_rows {
            if applied.insert(c.signature_hash.clone()) {
                apply_count += 1;
            }
        }
        assert_eq!(apply_count, before, "re-pulled rows dedup, no double-apply");
    }
}
