//! Generates a legacy database fixture from the current INITIAL_SCHEMA.
//! Run with: cargo test -p androidqa-core -- generate_legacy_fixture --ignored
//!
//! The fixture is committed to crates/apiqa-core/tests/fixtures/legacy_schema.db
//! and used in the migration engine tests (PR 2) to prove backward compatibility.
//!
//! We apply INITIAL_SCHEMA directly via rusqlite (not via Database::open) so the
//! fixture represents exactly what a production database looks like — tables
//! created by idempotent DDL, no schema_migrations row for the migration engine.

use rusqlite::Connection;

#[test]
#[ignore = "run manually to regenerate the fixture"]
fn generate_legacy_fixture() {
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    let fixture_path = fixture_dir.join("legacy_schema.db");
    let _ = std::fs::remove_file(&fixture_path);

    let conn = Connection::open(&fixture_path).unwrap();

    // Apply the current INITIAL_SCHEMA — same DDL as a production database.
    conn.execute_batch(androidqa_core::persistence::migrations::INITIAL_SCHEMA)
        .unwrap();

    // Insert test data into legacy dormant tables so PR 2 can verify
    // that migration preserves them.
    conn.execute(
        "INSERT INTO projects(id, name, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params!["p-legacy", "Legacy Project", "2025-01-01T00:00:00Z"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO environments(id, project_id, name) VALUES (?1, ?2, ?3)",
        rusqlite::params!["e-legacy", "p-legacy", "Legacy Env"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO devices(id, serial, metadata_json) VALUES (?1, ?2, ?3)",
        rusqlite::params!["d-legacy", "legacy-serial", "{}"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions(id, project_id, device_id, package_name, app_version, environment_id, status, started_at, ended_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            "s-legacy",
            "p-legacy",
            "d-legacy",
            "com.example.app",
            "1.0.0",
            "e-legacy",
            "completed",
            "2025-01-01T00:00:00Z",
            "2025-01-01T01:00:00Z",
        ],
    )
    .unwrap();
    conn.close().unwrap();

    let meta = std::fs::metadata(&fixture_path).unwrap();
    println!(
        "Fixture written to {} ({} bytes)",
        fixture_path.display(),
        meta.len()
    );
}
