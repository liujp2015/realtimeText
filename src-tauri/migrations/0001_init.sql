-- Sessions: one continuous recognition cycle
CREATE TABLE IF NOT EXISTS sessions (
    guid         TEXT PRIMARY KEY,
    started_at   BIGINT NOT NULL,
    ended_at     BIGINT NULL,
    device_name  TEXT NOT NULL
);

-- Transcriptions: final (stable) subtitle lines
CREATE TABLE IF NOT EXISTS transcriptions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_guid    TEXT NOT NULL,
    text            TEXT NOT NULL,
    start_ts        BIGINT NOT NULL,
    end_ts          BIGINT NOT NULL,
    paralinguistic  TEXT NULL,
    created_at      BIGINT NOT NULL DEFAULT (strftime('%s','now')),
    FOREIGN KEY (session_guid) REFERENCES sessions(guid) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_transcriptions_session_guid ON transcriptions(session_guid);
CREATE INDEX IF NOT EXISTS idx_transcriptions_created_at ON transcriptions(created_at);
CREATE INDEX IF NOT EXISTS idx_transcriptions_text ON transcriptions(text);

-- App config: key-value store
CREATE TABLE IF NOT EXISTS app_config (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
