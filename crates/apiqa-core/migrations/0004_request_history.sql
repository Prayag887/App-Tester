-- Migration: 0004_request_history
-- Composer request history (deduplicated by method + url + body).

CREATE TABLE IF NOT EXISTS history (
    id TEXT PRIMARY KEY,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    request_json TEXT NOT NULL,
    status INTEGER,
    sent_at TEXT NOT NULL,
    UNIQUE(method, url, request_json)
);

CREATE INDEX IF NOT EXISTS history_sent_at ON history(sent_at);
