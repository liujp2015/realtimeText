use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub guid: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListItem {
    pub guid: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub device_name: String,
    pub transcription_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionRow {
    pub id: i64,
    pub session_guid: String,
    pub text: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub paralinguistic: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub id: i64,
    pub session_guid: String,
    pub session_started_at: i64,
    pub text: String,
    pub start_ts: i64,
    pub end_ts: i64,
}

pub async fn insert_session(
    pool: &SqlitePool,
    guid: &str,
    started_at: i64,
    device_name: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO sessions (guid, started_at, device_name) VALUES (?, ?, ?)",
    )
    .bind(guid)
    .bind(started_at)
    .bind(device_name)
    .execute(pool)
    .await
    .context("insert_session")?;
    Ok(())
}

pub async fn finalize_session(pool: &SqlitePool, guid: &str, ended_at: i64) -> Result<()> {
    sqlx::query("UPDATE sessions SET ended_at = ? WHERE guid = ?")
        .bind(ended_at)
        .bind(guid)
        .execute(pool)
        .await
        .context("finalize_session")?;
    Ok(())
}

pub async fn insert_transcription(
    pool: &SqlitePool,
    session_guid: &str,
    text: &str,
    start_ts: i64,
    end_ts: i64,
    paralinguistic: Option<&str>,
) -> Result<i64> {
    let res = sqlx::query(
        "INSERT INTO transcriptions (session_guid, text, start_ts, end_ts, paralinguistic) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(session_guid)
    .bind(text)
    .bind(start_ts)
    .bind(end_ts)
    .bind(paralinguistic)
    .execute(pool)
    .await
    .context("insert_transcription")?;
    Ok(res.last_insert_rowid())
}

pub async fn list_sessions(
    pool: &SqlitePool,
    limit: i64,
    offset: i64,
) -> Result<(i64, Vec<SessionListItem>)> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(pool)
        .await
        .context("count sessions")?;

    let rows = sqlx::query(
        "SELECT s.guid, s.started_at, s.ended_at, s.device_name, \
                (SELECT COUNT(*) FROM transcriptions t WHERE t.session_guid = s.guid) AS cnt \
         FROM sessions s \
         ORDER BY s.started_at DESC \
         LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .map(|row: SqliteRow| SessionListItem {
        guid: row.get("guid"),
        started_at: row.get("started_at"),
        ended_at: row.get("ended_at"),
        device_name: row.get("device_name"),
        transcription_count: row.get("cnt"),
    })
    .fetch_all(pool)
    .await
    .context("list_sessions")?;

    Ok((total, rows))
}

pub async fn get_session_with_transcriptions(
    pool: &SqlitePool,
    guid: &str,
) -> Result<(Option<SessionRow>, Vec<TranscriptionRow>)> {
    let session = sqlx::query("SELECT guid, started_at, ended_at, device_name FROM sessions WHERE guid = ?")
        .bind(guid)
        .map(|row: SqliteRow| SessionRow {
            guid: row.get("guid"),
            started_at: row.get("started_at"),
            ended_at: row.get("ended_at"),
            device_name: row.get("device_name"),
        })
        .fetch_optional(pool)
        .await
        .context("get_session")?;

    let transcriptions = sqlx::query(
        "SELECT id, session_guid, text, start_ts, end_ts, paralinguistic, created_at \
         FROM transcriptions WHERE session_guid = ? ORDER BY start_ts ASC",
    )
    .bind(guid)
    .map(|row: SqliteRow| TranscriptionRow {
        id: row.get("id"),
        session_guid: row.get("session_guid"),
        text: row.get("text"),
        start_ts: row.get("start_ts"),
        end_ts: row.get("end_ts"),
        paralinguistic: row.get("paralinguistic"),
        created_at: row.get("created_at"),
    })
    .fetch_all(pool)
    .await
    .context("get_session transcriptions")?;

    Ok((session, transcriptions))
}

pub async fn delete_session(pool: &SqlitePool, guid: &str) -> Result<()> {
    // ON DELETE CASCADE handles transcriptions
    sqlx::query("DELETE FROM sessions WHERE guid = ?")
        .bind(guid)
        .execute(pool)
        .await
        .context("delete_session")?;
    Ok(())
}

pub async fn clear_history(pool: &SqlitePool) -> Result<()> {
    sqlx::query("DELETE FROM transcriptions")
        .execute(pool)
        .await
        .context("clear transcriptions")?;
    sqlx::query("DELETE FROM sessions")
        .execute(pool)
        .await
        .context("clear sessions")?;
    Ok(())
}

pub async fn search_keywords(pool: &SqlitePool, keyword: &str) -> Result<Vec<SearchResultItem>> {
    let pattern = format!("%{}%", keyword);
    let rows = sqlx::query(
        "SELECT t.id, t.session_guid, s.started_at AS session_started_at, t.text, t.start_ts, t.end_ts \
         FROM transcriptions t \
         JOIN sessions s ON t.session_guid = s.guid \
         WHERE t.text LIKE ? \
         ORDER BY t.start_ts DESC \
         LIMIT 200",
    )
    .bind(pattern)
    .map(|row: SqliteRow| SearchResultItem {
        id: row.get("id"),
        session_guid: row.get("session_guid"),
        session_started_at: row.get("session_started_at"),
        text: row.get("text"),
        start_ts: row.get("start_ts"),
        end_ts: row.get("end_ts"),
    })
    .fetch_all(pool)
    .await
    .context("search_keywords")?;
    Ok(rows)
}

pub async fn get_config_value(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    let v: Option<String> = sqlx::query_scalar("SELECT value FROM app_config WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .context("get_config_value")?;
    Ok(v)
}

pub async fn set_config_value(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO app_config (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .context("set_config_value")?;
    Ok(())
}
