CREATE TABLE IF NOT EXISTS collections (id TEXT PRIMARY KEY, name TEXT NOT NULL, data TEXT NOT NULL, imported_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, collection_id TEXT NOT NULL, started_at TEXT NOT NULL, state TEXT NOT NULL, data TEXT NOT NULL, FOREIGN KEY(collection_id) REFERENCES collections(id) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_runs_collection_started ON runs(collection_id, started_at DESC);
CREATE TABLE IF NOT EXISTS environments (id TEXT PRIMARY KEY, name TEXT NOT NULL, data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS response_blobs (hash TEXT PRIMARY KEY, compressed BLOB NOT NULL, original_bytes INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS run_executions (run_id TEXT NOT NULL, position INTEGER NOT NULL, data TEXT NOT NULL, PRIMARY KEY(run_id, position), FOREIGN KEY(run_id) REFERENCES runs(id) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS comparison_rules (id TEXT NOT NULL, version INTEGER NOT NULL, scope_id TEXT NOT NULL, created_at TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(id, version));
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
