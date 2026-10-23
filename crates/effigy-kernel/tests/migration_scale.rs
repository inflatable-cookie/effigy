//! Operator-copy qualification runs only through the explicit private-VM selector.
mod support;
use effigy_kernel::{ctx::*, id::*, migration, root::StateRoot};
use rusqlite::{params_from_iter, types::Value, Connection, OpenFlags};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn hash(path: &Path) -> String {
    let mut file = std::fs::File::open(path).unwrap();
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn readonly(path: &Path) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            readonly(&entry.unwrap().path());
        }
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o500)).unwrap();
    } else {
        let executable = std::fs::metadata(path).unwrap().permissions().mode() & 0o100;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o400 | executable))
            .unwrap();
    }
}
fn writable(path: &Path) {
    if path.is_dir() {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        for entry in std::fs::read_dir(path).unwrap() {
            writable(&entry.unwrap().path());
        }
    } else {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
fn copy_private(source: &Path, target: &Path) {
    let metadata = std::fs::symlink_metadata(source).unwrap();
    assert!(!metadata.file_type().is_symlink());
    if metadata.is_dir() {
        std::fs::create_dir(target).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            copy_private(&entry.path(), &target.join(entry.file_name()));
        }
    } else {
        assert!(metadata.is_file());
        std::fs::copy(source, target).unwrap();
        std::fs::set_permissions(
            target,
            std::fs::Permissions::from_mode(0o600 | metadata.permissions().mode() & 0o100),
        )
        .unwrap();
    }
}
fn read_database(path: &Path) -> Connection {
    Connection::open_with_flags(
        format!("file:{}?immutable=1", path.display()),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap()
}
fn migrate(label: &str, source_home: &Path) {
    let fixture = effigy_test_support::Fixture::new("q010-scale-dest-").unwrap();
    let machine = fixture.path().join("machine");
    std::fs::create_dir(&machine).unwrap();
    let op = support::operation(
        EffectClass::Mutate,
        Placement::Host,
        Duration::from_secs(600),
    );
    let observe = op.observe().unwrap();
    let source = StateRoot::observe(&observe, source_home)
        .verified()
        .unwrap()
        .clone();
    let database = source.path().join("kernel.db");
    let source_hash = hash(&database);
    let before = support::snapshot(source_home);
    let input = read_database(&database);
    let source_runs: i64 = input
        .query_row("SELECT count(*) FROM runs", [], |row| row.get(0))
        .unwrap();
    let source_claims: i64 = input
        .query_row(
            "SELECT count(*) FROM claims WHERE state<>'released'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let started = Instant::now();
    let plan = migration::plan(&observe, &source).unwrap();
    let expected_runs: usize = plan.partitions.iter().map(|part| part.runs.len()).sum();
    let mut groups = BTreeMap::new();
    let mut locations = Vec::new();
    for part in &plan.partitions {
        if let Some(checkout) = part.checkout {
            let index = groups.len();
            let key = part
                .path_hint
                .clone()
                .unwrap_or_else(|| format!("{checkout:?}"));
            let path = groups
                .entry(key)
                .or_insert_with(|| {
                    let path = fixture.path().join(format!("checkout-{index}"));
                    std::fs::create_dir(&path).unwrap();
                    path
                })
                .clone();
            // Explicit model mapping: never resolve or link an operator checkout.
            locations.push((
                checkout,
                migration::CheckoutLocation {
                    root: path,
                    git_directory: None,
                },
            ));
        }
    }
    let report = migration::import(
        &op.effects().unwrap(),
        &source,
        &machine,
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |id, _, _| {
            locations
                .iter()
                .find(|(stored, _)| *stored == id)
                .map(|(_, location)| location.clone())
                .ok_or_else(|| {
                    effigy_kernel::store::StoreError::Corrupt("unmapped private checkout".into())
                })
        },
    )
    .unwrap();
    let mut output_runs = 0i64;
    let mut output_claims = 0i64;
    let mut outputs = Vec::new();
    for destination in &report.receipt.destinations {
        let database = destination.checkout_root.join(".effigy/state").join(
            if destination.checkout.is_some() {
                "kernel.db"
            } else {
                "machine.db"
            },
        );
        let connection = read_database(&database);
        assert_eq!(
            connection
                .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert!(!connection
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap());
        let runs: i64 = connection
            .query_row("SELECT count(*) FROM runs", [], |row| row.get(0))
            .unwrap();
        let claims: i64 = connection
            .query_row("SELECT count(*) FROM claims", [], |row| row.get(0))
            .unwrap();
        output_runs += runs;
        output_claims += claims;
        let mut query = connection
            .prepare("SELECT name,desired,last_applied,state,backend,engine FROM resources")
            .unwrap();
        for row in query
            .query_map([], |row| {
                (0..6)
                    .map(|index| row.get::<_, Value>(index))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
        {
            let row = row.unwrap();
            let original = input.query_row("SELECT name,desired,last_applied,state,backend,engine FROM resources WHERE name=?1", [&row[0]],
                |row| (0..6).map(|index| row.get::<_, Value>(index)).collect::<rusqlite::Result<Vec<_>>>()).unwrap();
            assert_eq!(row, original, "opaque resource state changed");
        }
        outputs.push(serde_json::json!({"path":database,"bytes":std::fs::metadata(&database).unwrap().len(),
            "rows":destination.rows,"unresolved_runs":destination.unresolved_runs,"integrity":"ok"}));
    }
    assert_eq!(output_runs as usize, expected_runs);
    assert!(
        output_runs < source_runs,
        "unrelated completed history was retained"
    );
    assert_eq!(output_claims, source_claims);
    assert_eq!(hash(&database), source_hash);
    assert_eq!(
        support::snapshot(source_home),
        before,
        "read-only source mutated"
    );
    println!(
        "SCALE {}",
        serde_json::json!({"label":label,"source_bytes":std::fs::metadata(&database).unwrap().len(),
        "source_sha256":source_hash,"source_runs":source_runs,"imported_runs":output_runs,"claims":output_claims,
        "seconds":started.elapsed().as_secs_f64(),"import_ms":report.elapsed_ms,"outputs":outputs})
    );
}

#[test]
#[ignore = "explicit actual-copy acceptance selector; never the operator HOME"]
fn actual_copy_and_outage_scale_migrate_without_source_writes() {
    assert_eq!(
        std::env::consts::OS,
        "linux",
        "copied macOS process receipts need a foreign boot namespace"
    );
    let seed =
        PathBuf::from(std::env::var_os("EFFIGY_RUNTIME_COPY_HOME").expect("private copied HOME"));
    assert!(seed.starts_with("/runtime-copy"));
    // The host bind mount is virtiofs, correctly refused as a mutation
    // authority. Copy into the VM's local filesystem before qualification.
    let fixture = effigy_test_support::Fixture::new("q010-scale-source-").unwrap();
    let source = fixture.path().join("actual");
    copy_private(&seed, &source);
    readonly(&source);
    assert!(
        source
            .join(".effigy/state/kernel.db")
            .metadata()
            .unwrap()
            .len()
            >= 138_000_000
    );
    migrate("actual-read-only-copy", &source);
    let stress = fixture.path().join("stress");
    copy_private(&source, &stress);
    let state = stress.join(".effigy/state");
    let mut connection = Connection::open(state.join("kernel.db")).unwrap();
    let columns: Vec<String> = connection
        .prepare("PRAGMA table_info(runs)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let mut sample = connection
        .query_row(
            "SELECT * FROM runs WHERE phase='closed' AND cleanup_outcome='complete' LIMIT 1",
            [],
            |row| {
                (0..columns.len())
                    .map(|index| row.get::<_, Value>(index))
                    .collect::<rusqlite::Result<Vec<_>>>()
            },
        )
        .unwrap();
    let count: i64 = connection
        .query_row("SELECT count(*) FROM runs", [], |row| row.get(0))
        .unwrap();
    let id_column = columns.iter().position(|name| name == "run_id").unwrap();
    let parent_column = columns
        .iter()
        .position(|name| name == "parent_run")
        .unwrap();
    let sql = format!(
        "INSERT INTO runs ({}) VALUES ({})",
        columns
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(","),
        vec!["?"; columns.len()].join(",")
    );
    let transaction = connection.transaction().unwrap();
    for index in count..18_700 {
        let mut entropy = [0; 10];
        entropy[..8].copy_from_slice(&(index as u64).to_be_bytes());
        sample[id_column] = Value::Text(RunId::new(1, entropy).unwrap().to_string());
        sample[parent_column] = Value::Null;
        transaction
            .execute(&sql, params_from_iter(&sample))
            .unwrap();
    }
    transaction.commit().unwrap();
    let gates: Vec<(String, String)> = connection
        .prepare("SELECT scope,name FROM resources ORDER BY name LIMIT 10800")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(gates.len(), 10_800);
    for (scope, name) in gates {
        let directory = state.join("gates").join(&scope);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(format!("{name}.gate")), []).unwrap();
        std::fs::write(state.join("gates").join(format!("{scope}.scope")), []).unwrap();
    }
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    drop(connection);
    readonly(&stress);
    migrate("outage-scale-18700-runs-10800-gates", &stress);
    writable(&stress);
    writable(&source);
}
