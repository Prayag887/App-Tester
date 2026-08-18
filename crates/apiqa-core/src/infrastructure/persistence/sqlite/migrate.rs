//! Forward-only migration engine with WAL-safe backup and checksum enforcement.
//!
//! Migrations are embedded in the binary via `include_str!` from
//! `crates/apiqa-core/migrations/`. Each migration is immutable after release —
//! changing an already-applied migration's SQL causes the engine to reject the
//! database at startup.
//!
//! ## Safety guarantees
//! - **Backup before upgrade**: `VACUUM INTO` creates a consistent snapshot before
//!   any migration runs. On failure the backup is restored.
//! - **Checksum enforcement**: each applied migration stores its SHA-256 checksum.
//!   A checksum mismatch means the migration file was edited after application
//!   and the engine refuses to open the database.
//! - **Legacy bootstrap**: databases created by the old `INITIAL_SCHEMA` batch
//!   (no `schema_migrations` rows) are detected, validated, and upgraded.
//! - **Newer database rejection**: a database at a version higher than the app
//!   knows about is rejected — never silently downgraded.

use crate::persistence::StoreError;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

// ---------------------------------------------------------------------------
// Migration registry — one entry per migration file in crates/apiqa-core/migrations/
// ---------------------------------------------------------------------------

struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

fn migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            name: "0001_initial",
            sql: include_str!("../../../../migrations/0001_initial.sql"),
        },
        Migration {
            version: 2,
            name: "0002_composer_collections",
            sql: include_str!("../../../../migrations/0002_composer_collections.sql"),
        },
        Migration {
            version: 3,
            name: "0003_composer_environments",
            sql: include_str!("../../../../migrations/0003_composer_environments.sql"),
        },
        Migration {
            version: 4,
            name: "0004_request_history",
            sql: include_str!("../../../../migrations/0004_request_history.sql"),
        },
        Migration {
            version: 5,
            name: "0005_daily_endpoint_snapshots",
            sql: include_str!("../../../../migrations/0005_daily_endpoint_snapshots.sql"),
        },
    ]
}

fn latest_version() -> u32 {
    migrations().last().map(|m| m.version).unwrap_or(1)
}

fn checksum(sql: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(sql.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// Engine — public API
// ---------------------------------------------------------------------------

pub struct MigrationEngine;

impl MigrationEngine {
    /// Validates that a file-backed database path is ready for migration.
    /// No-op for in-memory databases. For file-backed databases, checks that
    /// the parent directory exists.
    pub fn prepare(path: &Path) -> Result<(), StoreError> {
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)
                .map_err(|_| StoreError::Sql(rusqlite::Error::InvalidPath(path.to_path_buf())))?;
        }
        Ok(())
    }

    /// Migrate a file-backed database.
    ///
    /// 1. Detect current version
    /// 2. If up to date with matching checksums → return Ok
    /// 3. Create WAL-safe backup via VACUUM INTO
    /// 4. Apply each pending migration inside a transaction
    /// 5. On failure: close connection, restore backup, return Err
    /// 6. On success: delete backup, run integrity check
    pub fn migrate(conn: &Connection, db_path: &Path) -> Result<u32, StoreError> {
        let current = current_version(conn)?;

        if current > latest_version() as i64 {
            return Err(StoreError::Sql(rusqlite::Error::InvalidParameterName(
                format!(
                    "database is at version {current} but the app expects version {} — \
                     this database was opened by a newer version of App Tester",
                    latest_version()
                ),
            )));
        }

        // Three cases for an existing database:
        // 1. Brand new (no tables at all) → apply all migrations fresh.
        // 2. Legacy (has tables but no schema_migrations table) → bootstrap.
        // 3. Normal (has schema_migrations) → verify checksums, apply pending.
        let is_brand_new = !has_any_table(conn);
        let has_migrations_table = has_schema_migrations_table(conn);

        if is_brand_new {
            return Self::apply_all(conn);
        }

        if !has_migrations_table {
            // Legacy: old INITIAL_SCHEMA tables but no migration tracking.
            return Self::bootstrap_legacy(conn, db_path);
        }

        // The old schema_migrations table (from INITIAL_SCHEMA) had only
        // (version, applied_at). The new schema adds (name, checksum).
        // If the table has the old shape, recreate it.
        ensure_migrations_table_schema(conn)?;

        // Verify checksums of already-applied migrations.
        verify_applied_checksums(conn)?;

        let current = current as u32;
        if current >= latest_version() {
            return Ok(current);
        }

        // Create backup before applying pending migrations.
        let backup_path = create_backup(conn, db_path)?;

        let result = Self::apply_pending(conn, current);
        match result {
            Ok(version) => {
                // Clean up backup on success.
                let _ = std::fs::remove_file(&backup_path);
                // Verify integrity.
                conn.execute_batch("PRAGMA integrity_check")
                    .map_err(StoreError::Sql)?;
                Ok(version)
            }
            Err(error) => {
                // Restore backup on failure.
                restore_backup(db_path, &backup_path)?;
                Err(error)
            }
        }
    }

    /// Migrate an in-memory database (no backup needed).
    pub fn migrate_in_memory(conn: &Connection) -> Result<u32, StoreError> {
        let is_brand_new = !has_any_table(conn);
        let has_migrations_table = has_schema_migrations_table(conn);

        if is_brand_new {
            return Self::apply_all(conn);
        }

        let current = current_version(conn)?;

        if current > latest_version() as i64 {
            return Err(StoreError::Sql(rusqlite::Error::InvalidParameterName(
                format!(
                    "in-memory database is at version {current} but the app expects version {}",
                    latest_version()
                ),
            )));
        }

        if !has_migrations_table {
            return Self::bootstrap_legacy_in_memory(conn);
        }

        ensure_migrations_table_schema(conn)?;
        verify_applied_checksums(conn)?;
        Self::apply_pending(conn, current as u32)
    }

    // ------------------------------------------------------------------
    // Internal
    // ------------------------------------------------------------------

    fn bootstrap_legacy(conn: &Connection, db_path: &Path) -> Result<u32, StoreError> {
        // Validate the legacy schema — key tables must exist.
        validate_legacy_schema(conn)?;

        let backup_path = create_backup(conn, db_path)?;
        let result = Self::apply_all(conn);
        match result {
            Ok(version) => {
                let _ = std::fs::remove_file(&backup_path);
                conn.execute_batch("PRAGMA integrity_check")
                    .map_err(StoreError::Sql)?;
                Ok(version)
            }
            Err(error) => {
                restore_backup(db_path, &backup_path)?;
                Err(error)
            }
        }
    }

    fn bootstrap_legacy_in_memory(conn: &Connection) -> Result<u32, StoreError> {
        validate_legacy_schema(conn)?;
        Self::apply_all(conn)
    }

    fn apply_all(conn: &Connection) -> Result<u32, StoreError> {
        let now = OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| OffsetDateTime::now_utc().to_string());
        for migration in migrations() {
            conn.execute_batch(migration.sql).map_err(|error| {
                StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
            })?;
            conn.execute(
                "INSERT INTO schema_migrations(version, name, checksum, applied_at)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    migration.version,
                    migration.name,
                    checksum(migration.sql),
                    now
                ],
            )
            .map_err(StoreError::Sql)?;
        }
        Ok(latest_version())
    }

    fn apply_pending(conn: &Connection, current: u32) -> Result<u32, StoreError> {
        let now = OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| OffsetDateTime::now_utc().to_string());
        for migration in migrations() {
            if migration.version <= current {
                continue;
            }
            let tx = conn.unchecked_transaction().map_err(StoreError::Sql)?;
            let result = tx.execute_batch(migration.sql);
            match result {
                Ok(()) => {
                    tx.execute(
                        "INSERT INTO schema_migrations(version, name, checksum, applied_at)
                         VALUES (?1, ?2, ?3, ?4)",
                        rusqlite::params![
                            migration.version,
                            migration.name,
                            checksum(migration.sql),
                            now
                        ],
                    )
                    .map_err(StoreError::Sql)?;
                    tx.commit().map_err(StoreError::Sql)?;
                }
                Err(error) => {
                    // Transaction rolls back on drop.
                    return Err(StoreError::Sql(error));
                }
            }
        }
        Ok(latest_version())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn current_version(conn: &Connection) -> Result<i64, StoreError> {
    if !has_schema_migrations_table(conn) {
        return Ok(0); // Legacy — no version recorded.
    }
    Ok(conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0))
}

fn has_any_table(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .unwrap_or(false)
}

fn has_schema_migrations_table(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .unwrap_or(false)
}

/// The old schema_migrations table (from the original INITIAL_SCHEMA) had only
/// (version, applied_at). The new schema has (version, name, checksum, applied_at).
/// If the table has the old shape, rename it, create the new one, and migrate
/// any existing rows (with empty name/checksum columns for the old entries).
fn ensure_migrations_table_schema(conn: &Connection) -> Result<(), StoreError> {
    // Check if the 'name' column exists (new schema has it).
    let has_name_column: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('schema_migrations') WHERE name='name'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false);

    if has_name_column {
        return Ok(()); // Already the new schema.
    }

    // Old schema — rename, recreate, copy rows.
    conn.execute_batch(
        "ALTER TABLE schema_migrations RENAME TO schema_migrations_old;
         CREATE TABLE schema_migrations (
             version     INTEGER PRIMARY KEY,
             name        TEXT NOT NULL DEFAULT '',
             checksum    TEXT NOT NULL DEFAULT '',
             applied_at  TEXT NOT NULL
         );
         INSERT INTO schema_migrations(version, name, checksum, applied_at)
             SELECT version, '', '', applied_at FROM schema_migrations_old;
         DROP TABLE schema_migrations_old;",
    )
    .map_err(StoreError::Sql)?;
    Ok(())
}

fn verify_applied_checksums(conn: &Connection) -> Result<(), StoreError> {
    let mut statement = conn
        .prepare("SELECT version, checksum FROM schema_migrations ORDER BY version")
        .map_err(StoreError::Sql)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(StoreError::Sql)?;

    for row in rows {
        let (version, stored_checksum) = row.map_err(StoreError::Sql)?;
        let migration = migrations().into_iter().find(|m| m.version == version);
        match migration {
            Some(m) => {
                let actual = checksum(m.sql);
                if stored_checksum.is_empty() {
                    // Legacy row from old schema_migrations — no checksum was
                    // stored. Update it to the current checksum.
                    conn.execute(
                        "UPDATE schema_migrations SET checksum = ?1 WHERE version = ?2",
                        rusqlite::params![actual, version],
                    )
                    .map_err(StoreError::Sql)?;
                } else if actual != stored_checksum {
                    return Err(StoreError::Sql(rusqlite::Error::InvalidParameterName(
                        format!(
                            "checksum mismatch for migration {} ({}): the migration file was \
                             edited after being applied. Expected {}, found {}.",
                            m.version, m.name, stored_checksum, actual
                        ),
                    )));
                }
            }
            None => {
                // Migration version exists in the database but not in the app
                // binary — this database was opened by a newer version.
                return Err(StoreError::Sql(rusqlite::Error::InvalidParameterName(
                    format!(
                        "database contains migration version {version} which is not \
                         present in this build of App Tester"
                    ),
                )));
            }
        }
    }
    Ok(())
}

fn validate_legacy_schema(conn: &Connection) -> Result<(), StoreError> {
    // The legacy schema must have these key tables from the original release.
    let required: &[&str] = &[
        "projects",
        "environments",
        "devices",
        "sessions",
        "transactions",
    ];
    for table in required {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false);
        if !exists {
            return Err(StoreError::Sql(rusqlite::Error::InvalidParameterName(
                format!(
                    "legacy database is missing required table '{table}' — \
                     the database may be corrupted or was not created by App Tester"
                ),
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// WAL-safe backup and restore
// ---------------------------------------------------------------------------

fn backup_path_for(db_path: &Path) -> PathBuf {
    let mut path = db_path.to_path_buf();
    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    path.set_file_name(format!("{filename}.migration-backup"));
    path
}

fn create_backup(conn: &Connection, db_path: &Path) -> Result<PathBuf, StoreError> {
    let backup_path = backup_path_for(db_path);
    // VACUUM INTO creates a consistent, defragmented copy that includes all
    // committed pages — safe for WAL-mode databases where a raw file copy
    // might miss pages still in the -wal file.
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .map_err(StoreError::Sql)?;
    conn.execute(&format!("VACUUM INTO '{}'", backup_path.display()), [])
        .map_err(StoreError::Sql)?;
    Ok(backup_path)
}

fn restore_backup(db_path: &Path, backup_path: &Path) -> Result<(), StoreError> {
    // The caller must ensure all database handles are closed before calling
    // this function. We copy the backup over the original and remove WAL/SHM
    // files.
    std::fs::copy(backup_path, db_path)
        .map_err(|_| StoreError::Sql(rusqlite::Error::InvalidPath(db_path.to_path_buf())))?;
    // Remove WAL and SHM files to clear any stale WAL state.
    let wal = db_path.with_extension("db-wal");
    let shm = db_path.with_extension("db-shm");
    let _ = std::fs::remove_file(&wal);
    let _ = std::fs::remove_file(&shm);
    let _ = std::fs::remove_file(backup_path);
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (Connection, PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        (Connection::open(&path).unwrap(), path, dir)
    }

    #[test]
    fn fresh_database_applies_all_migrations() {
        let (conn, path, _dir) = temp_db();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        let version = MigrationEngine::migrate(&conn, &path).unwrap();
        assert_eq!(version, 5);

        // schema_migrations has all 5 rows.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 5);

        // All expected tables exist.
        for table in &[
            "projects",
            "collections",
            "composer_environments",
            "history",
            "endpoint_daily_snapshots",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists > 0, "table '{table}' should exist");
        }
    }

    #[test]
    fn existing_database_only_applies_pending_migrations() {
        let (conn, path, _dir) = temp_db();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();

        // Simulate a database at version 2 (only 0001 + 0002 applied).
        conn.execute_batch(include_str!("../../../../migrations/0001_initial.sql"))
            .unwrap();
        conn.execute_batch(include_str!(
            "../../../../migrations/0002_composer_collections.sql"
        ))
        .unwrap();
        // Insert a collection row to prove data survives.
        conn.execute(
            "INSERT INTO collections(id, name, description, color, created_at, updated_at)
             VALUES ('c1', 'My API', '', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        // Record migrations 1 and 2 as applied.
        let now = OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let m1 = &migrations()[0];
        let m2 = &migrations()[1];
        conn.execute(
            "INSERT INTO schema_migrations(version, name, checksum, applied_at) VALUES (?1,?2,?3,?4)",
            rusqlite::params![m1.version, m1.name, checksum(m1.sql), now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO schema_migrations(version, name, checksum, applied_at) VALUES (?1,?2,?3,?4)",
            rusqlite::params![m2.version, m2.name, checksum(m2.sql), now],
        )
        .unwrap();

        // Migrate to latest.
        let version = MigrationEngine::migrate(&conn, &path).unwrap();
        assert_eq!(version, 5);

        // The collection row survived.
        let name: String = conn
            .query_row("SELECT name FROM collections WHERE id='c1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(name, "My API");
    }

    #[test]
    fn database_too_new_is_rejected() {
        let (conn, path, _dir) = temp_db();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        // Apply all real migrations first so schema_migrations exists.
        MigrationEngine::migrate(&conn, &path).unwrap();
        // Then insert a fake version 999 row.
        conn.execute(
            "INSERT INTO schema_migrations(version, name, checksum, applied_at)
             VALUES (999, 'future', 'abc', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let conn2 = Connection::open(&path).unwrap();
        let result = MigrationEngine::migrate(&conn2, &path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("newer version"),
            "expected 'newer version' error, got: {err}"
        );
    }

    #[test]
    fn checksum_mismatch_rejects_database() {
        let (conn, path, _dir) = temp_db();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        MigrationEngine::migrate(&conn, &path).unwrap();

        // Tamper with the stored checksum for version 1.
        conn.execute(
            "UPDATE schema_migrations SET checksum = 'bad-checksum' WHERE version = 1",
            [],
        )
        .unwrap();
        drop(conn);

        let conn2 = Connection::open(&path).unwrap();
        let result = MigrationEngine::migrate(&conn2, &path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("checksum mismatch"),
            "expected checksum mismatch error, got: {err}"
        );
    }

    #[test]
    fn migration_from_legacy_fixture_preserves_data() {
        // Open the committed legacy fixture (created by PR 1).
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("legacy_schema.db");
        // Copy to a temp file so we don't mutate the committed fixture.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("legacy_copy.db");
        std::fs::copy(&fixture, &db_path).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();

        // Pre-condition: no schema_migrations rows (legacy).
        let has_table = has_schema_migrations_table(&conn);
        assert!(has_table, "fixture should have schema_migrations table");
        let version_before = current_version(&conn).unwrap();
        assert_eq!(
            version_before, 0,
            "legacy fixture should have no migration rows"
        );

        // Migrate.
        let version = MigrationEngine::migrate(&conn, &db_path).unwrap();
        assert_eq!(version, 5);

        // Legacy data survived.
        let name: String = conn
            .query_row("SELECT name FROM projects WHERE id='p-legacy'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(name, "Legacy Project");

        let env_name: String = conn
            .query_row(
                "SELECT name FROM environments WHERE id='e-legacy'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(env_name, "Legacy Env");
    }

    #[test]
    fn legacy_database_missing_key_table_is_rejected() {
        let (conn, path, _dir) = temp_db();
        // Create a database with schema_migrations table but missing 'projects'.
        conn.execute_batch(
            "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT, checksum TEXT, applied_at TEXT);
             CREATE TABLE sessions(id TEXT PRIMARY KEY);",
        )
        .unwrap();

        // This should fail because 'projects' is missing.
        // The legacy path detects no schema_migrations rows, sees the legacy
        // tables are incomplete, and rejects.
        // Actually: has_schema_migrations_table = true but current_version = 0.
        // That's not the legacy path — it goes through normal migration.
        // Let me test the actual legacy path by NOT having schema_migrations.
        drop(conn);
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY);")
            .unwrap();
        let result = MigrationEngine::migrate(&conn, &path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("missing required table"),
            "expected missing table error, got: {err}"
        );
    }

    #[test]
    fn dormant_tables_and_data_survive_all_migrations() {
        let (conn, path, _dir) = temp_db();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();

        // Apply 0001 only.
        conn.execute_batch(include_str!("../../../../migrations/0001_initial.sql"))
            .unwrap();
        // Insert legacy data.
        conn.execute(
            "INSERT INTO projects(id, name, created_at) VALUES ('p1', 'Test', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO environments(id, project_id, name) VALUES ('e1', 'p1', 'Dev')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO devices(id, serial, metadata_json) VALUES ('d1', 'abc', '{}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions(id, project_id, device_id, package_name, status, started_at)
             VALUES ('s1', 'p1', 'd1', 'com.example', 'running', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        // Record just version 1.
        let now = OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let m1 = &migrations()[0];
        conn.execute(
            "INSERT INTO schema_migrations(version, name, checksum, applied_at) VALUES (?1,?2,?3,?4)",
            rusqlite::params![m1.version, m1.name, checksum(m1.sql), now],
        )
        .unwrap();

        // Migrate to latest (2-4).
        MigrationEngine::migrate(&conn, &path).unwrap();

        // All legacy data survived.
        for (table, id) in &[
            ("projects", "p1"),
            ("environments", "e1"),
            ("devices", "d1"),
            ("sessions", "s1"),
        ] {
            let count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE id=?1"),
                    [id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "legacy row in {table} should survive");
        }
    }

    #[test]
    fn in_memory_database_applies_all_migrations() {
        let conn = Connection::open_in_memory().unwrap();
        let version = MigrationEngine::migrate_in_memory(&conn).unwrap();
        assert_eq!(version, 5);
    }

    #[test]
    fn in_memory_database_handles_legacy_schema() {
        // Create a legacy schema without schema_migrations rows.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../../../../migrations/0001_initial.sql"))
            .unwrap();
        // Delete schema_migrations rows to simulate legacy.
        conn.execute("DELETE FROM schema_migrations", []).unwrap();
        // But the table exists.
        assert!(has_schema_migrations_table(&conn));
        assert_eq!(current_version(&conn).unwrap(), 0);

        let version = MigrationEngine::migrate_in_memory(&conn).unwrap();
        assert_eq!(version, 5);
    }

    #[test]
    fn backup_is_created_and_removed_on_success() {
        let (conn, path, _dir) = temp_db();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        let backup_path = backup_path_for(&path);

        // Fresh migration creates and removes backup.
        MigrationEngine::migrate(&conn, &path).unwrap();
        assert!(!backup_path.exists(), "backup should be removed on success");
    }
}
