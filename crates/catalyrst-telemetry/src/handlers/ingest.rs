use std::time::Duration;

use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

const CHUNK_ROWS: usize = 256;
const FLUSH_EVERY: Duration = Duration::from_millis(200);
const QUEUE_CAP: usize = 10_000;

pub struct Row {
    pub source: &'static str,
    pub project: String,
    pub kind: String,
    pub body: Value,
    pub invalid_reason: Option<String>,
}

enum Msg {
    Row(Row),
    Flush(oneshot::Sender<()>),
}

/// Coalesces single-event writes into one INSERT per `FLUSH_EVERY` / `CHUNK_ROWS`; a full queue
/// degrades to a direct insert rather than dropping the event.
pub struct WriteBuffer {
    pool: PgPool,
    tx: mpsc::Sender<Msg>,
}

impl WriteBuffer {
    pub fn start(pool: PgPool) -> Self {
        let (tx, rx) = mpsc::channel(QUEUE_CAP);
        tokio::spawn(run(pool.clone(), rx));
        Self { pool, tx }
    }

    pub async fn push(&self, row: Row) {
        if let Err(e) = self.tx.try_send(Msg::Row(row)) {
            if let Msg::Row(row) = e.into_inner() {
                insert(&self.pool, std::slice::from_ref(&row)).await;
            }
        }
    }

    pub async fn push_all(&self, rows: Vec<Row>) {
        for row in rows {
            self.push(row).await;
        }
    }

    /// Resolves once every row queued before the call has been written.
    pub async fn flush(&self) {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(Msg::Flush(tx)).await.is_ok() {
            let _ = rx.await;
        }
    }
}

async fn run(pool: PgPool, mut rx: mpsc::Receiver<Msg>) {
    let mut rows: Vec<Row> = Vec::with_capacity(CHUNK_ROWS);
    let mut flushes: Vec<oneshot::Sender<()>> = Vec::new();
    loop {
        let Some(mut msg) = rx.recv().await else {
            break;
        };
        let deadline = Instant::now() + FLUSH_EVERY;
        let mut open = true;
        loop {
            match msg {
                Msg::Row(row) => rows.push(row),
                Msg::Flush(done) => flushes.push(done),
            }
            if !flushes.is_empty() || rows.len() >= CHUNK_ROWS {
                break;
            }
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(next)) => msg = next,
                Ok(None) => {
                    open = false;
                    break;
                }
                Err(_) => break,
            }
        }
        insert(&pool, &rows).await;
        rows.clear();
        for done in flushes.drain(..) {
            let _ = done.send(());
        }
        if !open {
            break;
        }
    }
}

/// One unnest INSERT per `CHUNK_ROWS`; a chunk the database rejects retries row by row so a
/// single bad body only loses itself.
pub async fn insert(pool: &PgPool, rows: &[Row]) {
    for chunk in rows.chunks(CHUNK_ROWS) {
        if chunk.len() > 1
            && !matches!(
                insert_chunk(pool, chunk).await,
                Err(sqlx::Error::Database(_))
            )
        {
            continue;
        }
        for row in chunk {
            let _ = insert_chunk(pool, std::slice::from_ref(row)).await;
        }
    }
}

async fn insert_chunk(pool: &PgPool, rows: &[Row]) -> Result<(), sqlx::Error> {
    let sources: Vec<&str> = rows.iter().map(|r| r.source).collect();
    let projects: Vec<&str> = rows.iter().map(|r| r.project.as_str()).collect();
    let kinds: Vec<&str> = rows.iter().map(|r| r.kind.as_str()).collect();
    let bodies: Vec<&Value> = rows.iter().map(|r| &r.body).collect();
    let reasons: Vec<Option<&str>> = rows.iter().map(|r| r.invalid_reason.as_deref()).collect();
    sqlx::query(
        "INSERT INTO telemetry_events (source, project, event_kind, body, invalid_reason) \
         SELECT * FROM unnest($1::text[], $2::text[], $3::text[], $4::jsonb[], $5::text[])",
    )
    .bind(&sources)
    .bind(&projects)
    .bind(&kinds)
    .bind(&bodies)
    .bind(&reasons)
    .execute(pool)
    .await
    .map(|_| ())
}
