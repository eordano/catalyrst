use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::http::ApiError;
use crate::ports::user_bans::ms_iso;

pub const MAX_EVIDENCE_FILES: usize = 5;
pub const MAX_EVIDENCE_BYTES: i64 = 10 * 1024 * 1024;
pub const MAX_PENDING_EVIDENCE_SLOTS: i64 = 25;
pub const MAX_REPORTS_PER_HOUR: i64 = 20;
pub const MAX_DESCRIPTION_CHARS: usize = 500;
pub const MAX_COMMENTS_CHARS: usize = 500;
pub const MAX_REASON_CHARS: usize = 64;
pub const MAX_FILENAME_CHARS: usize = 200;

pub const ALLOWED_EVIDENCE_CONTENT_TYPES: [&str; 7] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "video/mp4",
    "video/webm",
    "application/pdf",
];

pub const STATUS_OPEN: &str = "open";

#[derive(Debug, Serialize)]
pub struct PlayerReport {
    pub id: String,
    #[serde(rename = "reporterAddress")]
    pub reporter_address: String,
    #[serde(rename = "reportedAddress")]
    pub reported_address: String,
    pub reason: String,
    pub description: String,
    #[serde(rename = "additionalComments")]
    pub additional_comments: Option<String>,
    #[serde(rename = "evidenceKeys")]
    pub evidence_keys: Vec<String>,
    pub status: String,
    #[serde(rename = "createdAt")]
    #[serde(serialize_with = "ms_iso::serialize")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct EvidenceSlot {
    pub key: String,
    pub filename: String,
    #[serde(rename = "contentType")]
    pub content_type: String,
    #[serde(rename = "fileSize")]
    pub file_size: i64,
    pub uploaded: bool,
}

pub struct EvidenceRequest {
    pub filename: String,
    pub content_type: String,
    pub file_size: i64,
}

pub struct CreateReport {
    pub report_id: Uuid,
    pub reporter: String,
    pub reported: String,
    pub reason: String,
    pub description: String,
    pub additional_comments: Option<String>,
    pub evidence_keys: Vec<String>,
}

pub struct EvidenceBlob {
    pub filename: String,
    pub content_type: String,
    pub content: Vec<u8>,
}

#[derive(Debug)]
pub enum PresignError {
    TooManyPending,
    Db(ApiError),
}

#[derive(Debug)]
pub enum EvidenceUploadError {
    UnknownSlot,
    NotOwner,
    AlreadyUploaded,
    ContentTypeMismatch { expected: String },
    SizeMismatch { expected: i64, actual: i64 },
    Db(ApiError),
}

#[derive(Debug)]
pub enum ReportWriteError {
    UnknownReport,
    NotOwner,
    UnknownEvidence(String),
    EvidenceMissing(String),
    AlreadySubmitted,
    RateLimited,
    Db(ApiError),
}

impl From<sqlx::Error> for PresignError {
    fn from(e: sqlx::Error) -> Self {
        PresignError::Db(ApiError::from(e))
    }
}

impl From<sqlx::Error> for EvidenceUploadError {
    fn from(e: sqlx::Error) -> Self {
        EvidenceUploadError::Db(ApiError::from(e))
    }
}

impl From<sqlx::Error> for ReportWriteError {
    fn from(e: sqlx::Error) -> Self {
        ReportWriteError::Db(ApiError::from(e))
    }
}

pub fn is_allowed_content_type(value: &str) -> bool {
    let media = value
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    ALLOWED_EVIDENCE_CONTENT_TYPES.contains(&media.as_str())
}

pub fn evidence_key(index: usize, filename: &str) -> String {
    let sanitized: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let mut collapsed = sanitized;
    while collapsed.contains("..") {
        collapsed = collapsed.replace("..", ".");
    }
    let trimmed = collapsed.trim_matches('.');
    let name = if trimmed.is_empty() { "file" } else { trimmed };
    format!("{index}-{name}")
}

type ReportRow = (
    Uuid,
    String,
    String,
    String,
    String,
    Option<String>,
    Vec<String>,
    String,
    NaiveDateTime,
);

type ReportPageRow = (
    Uuid,
    String,
    String,
    String,
    String,
    Option<String>,
    Vec<String>,
    String,
    NaiveDateTime,
    i64,
);

const REPORT_SELECT_FIELDS: &str = "id, reporter_address, reported_address, reason, description, additional_comments, evidence_keys, status, created_at";

fn report_from_row(row: ReportRow) -> PlayerReport {
    let (
        id,
        reporter_address,
        reported_address,
        reason,
        description,
        additional_comments,
        evidence_keys,
        status,
        created_at,
    ) = row;
    PlayerReport {
        id: id.to_string(),
        reporter_address,
        reported_address,
        reason,
        description,
        additional_comments,
        evidence_keys,
        status,
        created_at: DateTime::from_naive_utc_and_offset(created_at, Utc),
    }
}

pub struct PlayerReportsComponent {
    pool: PgPool,
}

impl PlayerReportsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_evidence_slots(
        &self,
        reporter: &str,
        files: &[EvidenceRequest],
    ) -> Result<(Uuid, Vec<EvidenceSlot>), PresignError> {
        let reporter = reporter.to_lowercase();
        let report_id = Uuid::new_v4();
        let keys: Vec<String> = files
            .iter()
            .enumerate()
            .map(|(index, file)| evidence_key(index, &file.filename))
            .collect();
        let filenames: Vec<&str> = files.iter().map(|f| f.filename.as_str()).collect();
        let content_types: Vec<&str> = files.iter().map(|f| f.content_type.as_str()).collect();
        let sizes: Vec<i64> = files.iter().map(|f| f.file_size).collect();

        // The pending count excludes what `purged` removes, since both see one snapshot.
        let (pending, _inserted): (i64, i64) = sqlx::query_as(
            "WITH purged AS ( \
                DELETE FROM player_report_evidence e \
                WHERE e.created_at < now() - interval '24 hours' \
                  AND NOT EXISTS (SELECT 1 FROM player_reports r WHERE r.id = e.report_id) \
                RETURNING 1 \
             ), pending AS ( \
                SELECT count(*) AS n FROM player_report_evidence e \
                WHERE e.reporter_address = $1 \
                  AND e.created_at >= now() - interval '24 hours' \
                  AND NOT EXISTS (SELECT 1 FROM player_reports r WHERE r.id = e.report_id) \
             ), ins AS ( \
                INSERT INTO player_report_evidence \
                  (report_id, evidence_key, reporter_address, filename, content_type, declared_size) \
                SELECT $2, f.key, $1, f.filename, f.content_type, f.size \
                FROM unnest($3::text[], $4::text[], $5::text[], $6::int8[]) \
                     AS f(key, filename, content_type, size), pending \
                WHERE pending.n + $7 <= $8 \
                RETURNING 1 \
             ) \
             SELECT (SELECT n FROM pending), (SELECT count(*) FROM ins)",
        )
        .bind(&reporter)
        .bind(report_id)
        .bind(&keys)
        .bind(&filenames)
        .bind(&content_types)
        .bind(&sizes)
        .bind(files.len() as i64)
        .bind(MAX_PENDING_EVIDENCE_SLOTS)
        .fetch_one(&self.pool)
        .await?;
        if pending + files.len() as i64 > MAX_PENDING_EVIDENCE_SLOTS {
            return Err(PresignError::TooManyPending);
        }

        let slots = files
            .iter()
            .zip(keys)
            .map(|(file, key)| EvidenceSlot {
                key,
                filename: file.filename.clone(),
                content_type: file.content_type.clone(),
                file_size: file.file_size,
                uploaded: false,
            })
            .collect();

        Ok((report_id, slots))
    }

    pub async fn store_evidence(
        &self,
        report_id: Uuid,
        key: &str,
        uploader: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<EvidenceSlot, EvidenceUploadError> {
        let uploader = uploader.to_lowercase();
        let actual = bytes.len() as i64;
        let slot: Option<(String, String, String, i64, Option<NaiveDateTime>, bool)> =
            sqlx::query_as(
                "WITH slot AS ( \
                    SELECT reporter_address, filename, content_type, declared_size, uploaded_at \
                    FROM player_report_evidence WHERE report_id = $1 AND evidence_key = $2 \
                 ), upd AS ( \
                    UPDATE player_report_evidence e SET content = $3, uploaded_at = now() \
                    FROM slot \
                    WHERE e.report_id = $1 AND e.evidence_key = $2 AND e.uploaded_at IS NULL \
                      AND slot.reporter_address = $4 \
                      AND lower(slot.content_type) = lower($5) \
                      AND slot.declared_size = $6 \
                    RETURNING 1 \
                 ) \
                 SELECT reporter_address, filename, content_type, declared_size, uploaded_at, \
                        EXISTS (SELECT 1 FROM upd) \
                 FROM slot",
            )
            .bind(report_id)
            .bind(key)
            .bind(bytes)
            .bind(&uploader)
            .bind(content_type)
            .bind(actual)
            .fetch_optional(&self.pool)
            .await?;

        let Some((owner, filename, expected_type, declared_size, uploaded_at, updated)) = slot
        else {
            return Err(EvidenceUploadError::UnknownSlot);
        };
        if owner != uploader {
            return Err(EvidenceUploadError::NotOwner);
        }
        if uploaded_at.is_some() {
            return Err(EvidenceUploadError::AlreadyUploaded);
        }
        if !content_type.eq_ignore_ascii_case(&expected_type) {
            return Err(EvidenceUploadError::ContentTypeMismatch {
                expected: expected_type,
            });
        }
        if actual != declared_size {
            return Err(EvidenceUploadError::SizeMismatch {
                expected: declared_size,
                actual,
            });
        }
        if !updated {
            return Err(EvidenceUploadError::AlreadyUploaded);
        }

        Ok(EvidenceSlot {
            key: key.to_string(),
            filename,
            content_type: expected_type,
            file_size: declared_size,
            uploaded: true,
        })
    }

    pub async fn create_report(
        &self,
        input: CreateReport,
    ) -> Result<PlayerReport, ReportWriteError> {
        let reporter = input.reporter.to_lowercase();
        let reported = input.reported.to_lowercase();

        let mut txn = self.pool.begin().await?;

        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(&reporter)
            .execute(&mut *txn)
            .await?;

        // The SQL guard mirrors the checks below so nothing is written when one fails.
        type ReportInsertResult = (
            i64,
            Option<Vec<String>>,
            Option<Vec<String>>,
            Option<Vec<bool>>,
            Option<NaiveDateTime>,
        );
        let (recent, keys, owners, uploaded, created_at): ReportInsertResult = sqlx::query_as(
            "WITH recent AS ( \
                SELECT count(*) AS n FROM player_reports \
                WHERE reporter_address = $2 AND created_at > now() - interval '1 hour' \
             ), slots AS ( \
                SELECT evidence_key, reporter_address, uploaded_at FROM player_report_evidence \
                WHERE report_id = $1 ORDER BY evidence_key FOR UPDATE \
             ), ok AS ( \
                SELECT (SELECT n FROM recent) < $9 \
                   AND EXISTS (SELECT 1 FROM slots) \
                   AND NOT EXISTS (SELECT 1 FROM slots WHERE reporter_address <> $2) \
                   AND NOT EXISTS (SELECT 1 FROM unnest($7::text[]) AS k(key) \
                                   WHERE NOT EXISTS (SELECT 1 FROM slots s WHERE s.evidence_key = k.key)) \
                   AND NOT EXISTS (SELECT 1 FROM slots WHERE uploaded_at IS NULL) AS pass \
             ), ins AS ( \
                INSERT INTO player_reports \
                  (id, reporter_address, reported_address, reason, description, additional_comments, evidence_keys, status) \
                SELECT $1, $2, $3, $4, $5, $6, \
                       (SELECT array_agg(evidence_key ORDER BY evidence_key) FROM slots), $8 \
                FROM ok WHERE ok.pass \
                ON CONFLICT (id) DO NOTHING \
                RETURNING created_at \
             ) \
             SELECT (SELECT n FROM recent), \
                    (SELECT array_agg(evidence_key ORDER BY evidence_key) FROM slots), \
                    (SELECT array_agg(reporter_address ORDER BY evidence_key) FROM slots), \
                    (SELECT array_agg(uploaded_at IS NOT NULL ORDER BY evidence_key) FROM slots), \
                    (SELECT created_at FROM ins)",
        )
        .bind(input.report_id)
        .bind(&reporter)
        .bind(&reported)
        .bind(&input.reason)
        .bind(&input.description)
        .bind(&input.additional_comments)
        .bind(&input.evidence_keys)
        .bind(STATUS_OPEN)
        .bind(MAX_REPORTS_PER_HOUR)
        .fetch_one(&mut *txn)
        .await?;

        if recent >= MAX_REPORTS_PER_HOUR {
            return Err(ReportWriteError::RateLimited);
        }
        let keys = keys.unwrap_or_default();
        let owners = owners.unwrap_or_default();
        let uploaded = uploaded.unwrap_or_default();
        if keys.is_empty() {
            return Err(ReportWriteError::UnknownReport);
        }
        if owners.iter().any(|owner| owner != &reporter) {
            return Err(ReportWriteError::NotOwner);
        }
        for key in &input.evidence_keys {
            if !keys.contains(key) {
                return Err(ReportWriteError::UnknownEvidence(key.clone()));
            }
        }
        for (key, uploaded) in keys.iter().zip(&uploaded) {
            if !uploaded {
                return Err(ReportWriteError::EvidenceMissing(key.clone()));
            }
        }
        let Some(created_at) = created_at else {
            return Err(ReportWriteError::AlreadySubmitted);
        };

        txn.commit().await?;

        Ok(PlayerReport {
            id: input.report_id.to_string(),
            reporter_address: reporter,
            reported_address: reported,
            reason: input.reason,
            description: input.description,
            additional_comments: input.additional_comments,
            evidence_keys: keys,
            status: STATUS_OPEN.to_string(),
            created_at: DateTime::from_naive_utc_and_offset(created_at, Utc),
        })
    }

    pub async fn list_reports(
        &self,
        reported: Option<&str>,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PlayerReport>, ApiError> {
        let reported = reported.map(|a| a.to_lowercase());
        let rows = sqlx::query_as::<_, ReportRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {REPORT_SELECT_FIELDS} FROM player_reports \
             WHERE ($1::text IS NULL OR reported_address = $1) \
               AND ($2::text IS NULL OR status = $2) \
             ORDER BY created_at DESC LIMIT $3 OFFSET $4"
        )))
        .bind(reported)
        .bind(status)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(report_from_row).collect())
    }

    /// Page plus total in one statement; an empty page past the start falls back
    /// to a COUNT so the total stays exact.
    pub async fn list_reports_page(
        &self,
        reported: Option<&str>,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<PlayerReport>, i64), ApiError> {
        let lowered = reported.map(|a| a.to_lowercase());
        let rows = sqlx::query_as::<_, ReportPageRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {REPORT_SELECT_FIELDS}, count(*) OVER () FROM player_reports \
             WHERE ($1::text IS NULL OR reported_address = $1) \
               AND ($2::text IS NULL OR status = $2) \
             ORDER BY created_at DESC LIMIT $3 OFFSET $4"
        )))
        .bind(&lowered)
        .bind(status)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        let total = match rows.first() {
            Some(row) => row.9,
            None if offset == 0 => 0,
            None => self.count_reports(reported, status).await?,
        };
        let reports = rows
            .into_iter()
            .map(|(a, b, c, d, e, f, g, h, i, _)| report_from_row((a, b, c, d, e, f, g, h, i)))
            .collect();
        Ok((reports, total))
    }

    pub async fn count_reports(
        &self,
        reported: Option<&str>,
        status: Option<&str>,
    ) -> Result<i64, ApiError> {
        let reported = reported.map(|a| a.to_lowercase());
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM player_reports \
             WHERE ($1::text IS NULL OR reported_address = $1) \
               AND ($2::text IS NULL OR status = $2)",
        )
        .bind(reported)
        .bind(status)
        .fetch_one(&self.pool)
        .await?;
        Ok(total)
    }

    pub async fn get_report(&self, report_id: Uuid) -> Result<Option<PlayerReport>, ApiError> {
        let row = sqlx::query_as::<_, ReportRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {REPORT_SELECT_FIELDS} FROM player_reports WHERE id = $1"
        )))
        .bind(report_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(report_from_row))
    }

    pub async fn get_report_with_evidence(
        &self,
        report_id: Uuid,
    ) -> Result<Option<(PlayerReport, Vec<EvidenceSlot>)>, ApiError> {
        let (report, evidence) =
            tokio::try_join!(self.get_report(report_id), self.list_evidence(report_id))?;
        Ok(report.map(|report| (report, evidence)))
    }

    pub async fn list_evidence(&self, report_id: Uuid) -> Result<Vec<EvidenceSlot>, ApiError> {
        let rows: Vec<(String, String, String, i64, Option<NaiveDateTime>)> = sqlx::query_as(
            "SELECT evidence_key, filename, content_type, declared_size, uploaded_at \
             FROM player_report_evidence WHERE report_id = $1 ORDER BY evidence_key",
        )
        .bind(report_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(key, filename, content_type, file_size, uploaded_at)| EvidenceSlot {
                    key,
                    filename,
                    content_type,
                    file_size,
                    uploaded: uploaded_at.is_some(),
                },
            )
            .collect())
    }

    pub async fn load_evidence(
        &self,
        report_id: Uuid,
        key: &str,
    ) -> Result<Option<EvidenceBlob>, ApiError> {
        let row: Option<(String, String, Option<Vec<u8>>)> = sqlx::query_as(
            "SELECT filename, content_type, content FROM player_report_evidence \
             WHERE report_id = $1 AND evidence_key = $2",
        )
        .bind(report_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|(filename, content_type, content)| {
            content.map(|content| EvidenceBlob {
                filename,
                content_type,
                content,
            })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn evidence_key_neutralizes_path_traversal_and_separators() {
        assert_eq!(evidence_key(0, "shot.png"), "0-shot.png");
        assert_eq!(evidence_key(1, "../../etc/passwd"), "1-_._etc_passwd");
        assert_eq!(evidence_key(2, "a b?c=1.mp4"), "2-a_b_c_1.mp4");
        assert_eq!(evidence_key(3, "..."), "3-file");
        assert_eq!(evidence_key(4, ""), "4-file");
        for hostile in ["../../etc/passwd", "a/b\\c", "..%2f..", "x?y=1#z"] {
            let key = evidence_key(0, hostile);
            assert!(!key.contains('/'), "{key} must not carry a path separator");
            assert!(!key.contains(".."), "{key} must not carry a traversal");
        }
    }

    #[test]
    fn evidence_key_is_length_capped() {
        let long = "x".repeat(500);
        let key = evidence_key(0, &long);
        assert_eq!(key.len(), 82);
        assert!(key.starts_with("0-xxxx"));
    }

    #[test]
    fn content_type_allowlist_ignores_parameters_and_case() {
        assert!(is_allowed_content_type("image/png"));
        assert!(is_allowed_content_type("IMAGE/PNG; charset=binary"));
        assert!(is_allowed_content_type("video/mp4"));
        assert!(!is_allowed_content_type("text/html"));
        assert!(!is_allowed_content_type("application/x-sh"));
        assert!(!is_allowed_content_type(""));
    }

    #[test]
    fn report_serializes_camel_case_with_millis_timestamp() {
        let at = Utc.timestamp_opt(1_718_900_000, 0).unwrap();
        let v = serde_json::to_value(PlayerReport {
            id: "00000000-0000-0000-0000-000000000001".into(),
            reporter_address: "0xaaa".into(),
            reported_address: "0xbbb".into(),
            reason: "harassment".into(),
            description: "spam in chat".into(),
            additional_comments: None,
            evidence_keys: vec!["0-shot.png".into()],
            status: STATUS_OPEN.into(),
            created_at: at,
        })
        .unwrap();
        assert_eq!(v["id"], "00000000-0000-0000-0000-000000000001");
        assert_eq!(v["reporterAddress"], "0xaaa");
        assert_eq!(v["reportedAddress"], "0xbbb");
        assert_eq!(v["evidenceKeys"][0], "0-shot.png");
        assert_eq!(v["status"], "open");
        assert_eq!(v["createdAt"], "2024-06-20T16:13:20.000Z");
        assert!(v["additionalComments"].is_null());
    }
}
