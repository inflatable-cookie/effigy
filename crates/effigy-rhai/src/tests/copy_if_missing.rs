use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Barrier;
use std::time::{Duration, Instant};

fn context_at(name: &str) -> (PathBuf, ScriptContext) {
    let root = temp_root(name);
    let context = script_context(&root);
    (root, context)
}

fn copy_if_missing_via_rhai(context: &ScriptContext, source: &str, destination: &str) -> bool {
    copy_if_missing_via_rhai_named(context, source, destination, "copy-result.txt")
}

fn copy_if_missing_via_rhai_named(
    context: &ScriptContext,
    source: &str,
    destination: &str,
    result: &str,
) -> bool {
    let script = format!(
        r#"
        let won = fs::copy_if_missing({source}, {destination});
        if won {{ fs::write_file({result}, "true"); }}
        else {{ fs::write_file({result}, "false"); }}
        "#,
        source = serde_json::to_string(source).expect("source json"),
        destination = serde_json::to_string(destination).expect("destination json"),
        result = serde_json::to_string(result).expect("result json"),
    );
    execute_rhai_script(context, &script, &[], &callbacks()).expect("copy_if_missing execute");
    fs::read_to_string(context.cwd.join(result)).expect("read copy result") == "true"
}

fn staged_payload_leftovers(root: &Path) -> Vec<PathBuf> {
    let mut leftovers = Vec::new();
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.expect("walk staged payloads");
        if entry
            .file_name()
            .to_string_lossy()
            .contains(".effigy-publish-")
        {
            leftovers.push(entry.path().to_path_buf());
        }
    }
    leftovers
}

fn wait_for_staged_payload(root: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if !staged_payload_leftovers(root).is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for a staged payload under {}",
            root.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn assert_no_staging(root: &Path) {
    assert!(
        staged_payload_leftovers(root).is_empty(),
        "publication must retire every staged file under {}",
        root.display()
    );
}

#[test]
fn copy_if_missing_publishes_empty_and_large_payloads_once() {
    let (root, context) = context_at("fs-copy-if-missing-sizes");
    fs::write(root.join("empty.txt"), "").expect("empty source");
    let large = "Y".repeat(256 * 1024);
    fs::write(root.join("large.txt"), &large).expect("large source");

    assert!(copy_if_missing_via_rhai(
        &context,
        "empty.txt",
        "out/empty.txt"
    ));
    assert!(!copy_if_missing_via_rhai(
        &context,
        "empty.txt",
        "out/empty.txt"
    ));
    assert_eq!(
        fs::read_to_string(root.join("out/empty.txt")).expect("read empty dest"),
        ""
    );

    assert!(copy_if_missing_via_rhai(
        &context,
        "large.txt",
        "out/large.txt"
    ));
    assert!(!copy_if_missing_via_rhai(
        &context,
        "large.txt",
        "out/large.txt"
    ));
    assert_eq!(
        fs::read_to_string(root.join("out/large.txt")).expect("read large dest"),
        large
    );
    assert_no_staging(&root);
}

#[test]
fn copy_if_missing_resolves_relative_paths_from_cwd() {
    let (root, context) = context_at("fs-copy-if-missing-relative");
    fs::create_dir_all(root.join("src")).expect("src dir");
    fs::write(root.join("src/payload.txt"), "relative-bytes").expect("source");

    assert!(copy_if_missing_via_rhai(
        &context,
        "src/payload.txt",
        "nested/out/copy.txt"
    ));
    assert_eq!(
        fs::read_to_string(root.join("nested/out/copy.txt")).expect("read dest"),
        "relative-bytes"
    );
    assert_no_staging(&root);
}

#[test]
fn copy_if_missing_keeps_an_independent_copy_after_source_mutation() {
    let (root, context) = context_at("fs-copy-if-missing-independent");
    let source = root.join("source.txt");
    fs::write(&source, "original-payload").expect("source");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).expect("mode");
    }

    assert!(copy_if_missing_via_rhai(&context, "source.txt", "dest.txt"));
    fs::write(&source, "mutated-source").expect("mutate source");

    assert_eq!(
        fs::read_to_string(root.join("dest.txt")).expect("read dest"),
        "original-payload"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::PermissionsExt;
        let source_meta = fs::metadata(&source).expect("source metadata");
        let dest_meta = fs::metadata(root.join("dest.txt")).expect("dest metadata");
        assert_ne!(
            source_meta.ino(),
            dest_meta.ino(),
            "published copy must not alias the source inode"
        );
        assert_eq!(dest_meta.nlink(), 1, "staged hard link must be retired");
        assert_eq!(dest_meta.permissions().mode() & 0o777, 0o640);
    }
    assert_no_staging(&root);
}

#[test]
fn copy_if_missing_treats_file_and_directory_as_occupied() {
    let (root, context) = context_at("fs-copy-if-missing-occupied");
    fs::write(root.join("source.txt"), "fresh").expect("source");
    fs::write(root.join("existing.txt"), "keep-file").expect("existing file");
    fs::create_dir_all(root.join("existing-dir")).expect("existing dir");
    fs::write(root.join("existing-dir/child.txt"), "keep-dir").expect("dir child");

    assert!(!copy_if_missing_via_rhai(
        &context,
        "source.txt",
        "existing.txt"
    ));
    assert!(!copy_if_missing_via_rhai(
        &context,
        "source.txt",
        "existing-dir"
    ));
    assert_eq!(
        fs::read_to_string(root.join("existing.txt")).expect("file dest"),
        "keep-file"
    );
    assert_eq!(
        fs::read_to_string(root.join("existing-dir/child.txt")).expect("dir child"),
        "keep-dir"
    );
    assert_no_staging(&root);
}

#[cfg(unix)]
#[test]
fn copy_if_missing_treats_symlink_and_dangling_symlink_as_occupied() {
    let (root, context) = context_at("fs-copy-if-missing-symlink");
    fs::write(root.join("source.txt"), "replacement").expect("source");
    fs::write(root.join("target.txt"), "original").expect("target");
    std::os::unix::fs::symlink(root.join("target.txt"), root.join("link.txt")).expect("symlink");
    std::os::unix::fs::symlink(root.join("missing-target.txt"), root.join("dangling.txt"))
        .expect("dangling");

    assert!(!copy_if_missing_via_rhai(
        &context,
        "source.txt",
        "link.txt"
    ));
    assert!(!copy_if_missing_via_rhai(
        &context,
        "source.txt",
        "dangling.txt"
    ));
    assert_eq!(
        fs::read_to_string(root.join("target.txt")).expect("read target"),
        "original"
    );
    assert!(fs::symlink_metadata(root.join("link.txt"))
        .expect("link metadata")
        .file_type()
        .is_symlink());
    assert!(fs::symlink_metadata(root.join("dangling.txt"))
        .expect("dangling metadata")
        .file_type()
        .is_symlink());
    assert!(!root.join("missing-target.txt").exists());
    assert_no_staging(&root);
}

#[test]
fn copy_if_missing_returns_false_for_occupied_destination_without_reading_source() {
    let (root, context) = context_at("fs-copy-if-missing-occupied-unread-source");
    fs::write(root.join("existing.txt"), "keep-file").expect("existing file");
    fs::create_dir_all(root.join("existing-dir")).expect("existing dir");
    fs::write(root.join("existing-dir/child.txt"), "keep-dir").expect("dir child");
    fs::write(root.join("unreadable.txt"), "secret").expect("unreadable source");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            root.join("unreadable.txt"),
            fs::Permissions::from_mode(0o000),
        )
        .expect("chmod");
    }

    assert!(!copy_if_missing_via_rhai(
        &context,
        "absent-source.txt",
        "existing.txt"
    ));
    assert!(!copy_if_missing_via_rhai(
        &context,
        "absent-source.txt",
        "existing-dir"
    ));
    #[cfg(unix)]
    {
        assert!(!copy_if_missing_via_rhai(
            &context,
            "unreadable.txt",
            "existing.txt"
        ));
        std::os::unix::fs::symlink(root.join("missing-target.txt"), root.join("dangling.txt"))
            .expect("dangling");
        assert!(!copy_if_missing_via_rhai(
            &context,
            "absent-source.txt",
            "dangling.txt"
        ));
        assert!(fs::symlink_metadata(root.join("dangling.txt"))
            .expect("dangling metadata")
            .file_type()
            .is_symlink());
        fs::set_permissions(root.join("unreadable.txt"), {
            use std::os::unix::fs::PermissionsExt;
            fs::Permissions::from_mode(0o644)
        })
        .expect("restore mode");
    }
    assert_eq!(
        fs::read_to_string(root.join("existing.txt")).expect("file dest"),
        "keep-file"
    );
    assert_eq!(
        fs::read_to_string(root.join("existing-dir/child.txt")).expect("dir child"),
        "keep-dir"
    );
    assert_no_staging(&root);
}

#[test]
fn copy_if_missing_failure_leaves_no_destination_or_staging() {
    let (root, context) = context_at("fs-copy-if-missing-failure");
    fs::write(root.join("blocker"), "a file, not a directory").expect("blocker");
    fs::write(root.join("source.txt"), "payload").expect("source");

    let error = execute_rhai_script(
        &context,
        r#"fs::copy_if_missing("source.txt", "blocker/child.txt");"#,
        &[],
        &callbacks(),
    )
    .expect_err("publishing under a file parent must fail");
    assert!(
        error.to_string().contains("failed to write"),
        "failure must name the write path clearly: {error}"
    );
    assert!(!root.join("blocker/child.txt").exists());
    assert_no_staging(&root);

    let missing_source = execute_rhai_script(
        &context,
        r#"fs::copy_if_missing("absent-source.txt", "out/copy.txt");"#,
        &[],
        &callbacks(),
    )
    .expect_err("missing source must fail");
    assert!(
        missing_source.to_string().contains("failed to read"),
        "missing source must name the read path clearly: {missing_source}"
    );
    assert!(!root.join("out/copy.txt").exists());
    assert_no_staging(&root);
}

#[test]
fn exists_then_copy_overwrites_a_concurrent_winner() {
    let root = temp_root("fs-copy-if-missing-old-race");
    let source = root.join("loser.txt");
    let destination = root.join("dest.txt");
    fs::write(&source, "loser-payload").expect("loser source");

    let after_exists = Arc::new(Barrier::new(2));
    let allow_copy = Arc::new(Barrier::new(2));
    let destination_thread = destination.clone();
    let source_thread = source.clone();
    let after_exists_thread = Arc::clone(&after_exists);
    let allow_copy_thread = Arc::clone(&allow_copy);
    let loser = thread::spawn(move || {
        assert!(
            !destination_thread.exists(),
            "old-race control starts from an absent destination"
        );
        after_exists_thread.wait();
        allow_copy_thread.wait();
        fs::copy(&source_thread, &destination_thread).expect("old exists-then-copy");
    });

    after_exists.wait();
    fs::write(&destination, "winner-payload").expect("publish winner");
    allow_copy.wait();
    loser.join().expect("old-race loser join");
    assert_eq!(
        fs::read_to_string(&destination).expect("read overwritten dest"),
        "loser-payload",
        "the old exists-then-copy race must overwrite a concurrent winner"
    );
}

#[cfg(unix)]
#[test]
fn copy_if_missing_preserves_a_concurrent_winner_during_an_in_flight_copy() {
    let (root, context) = context_at("fs-copy-if-missing-fifo-race");
    let fifo = root.join("slow-source");
    let winner_source = root.join("winner.txt");
    fs::write(&winner_source, "winner-payload").expect("winner source");
    create_fifo(&fifo);

    let context = Arc::new(context);
    let loser_context = Arc::clone(&context);
    let loser = thread::spawn(move || {
        copy_if_missing_via_rhai_named(
            &loser_context,
            "slow-source",
            "dest.txt",
            "loser-result.txt",
        )
    });

    wait_for_staged_payload(&root);
    let winner_context = Arc::clone(&context);
    let winner = thread::spawn(move || {
        copy_if_missing_via_rhai_named(
            &winner_context,
            "winner.txt",
            "dest.txt",
            "winner-result.txt",
        )
    });
    assert!(
        winner.join().expect("winner join"),
        "complete copy must win"
    );

    let mut writer = fs::OpenOptions::new()
        .write(true)
        .open(&fifo)
        .expect("open fifo writer");
    writer
        .write_all(b"loser-payload-that-must-not-win")
        .expect("write fifo");
    drop(writer);

    assert!(
        !loser.join().expect("loser join"),
        "in-flight copy must lose once the winner is published"
    );
    assert_eq!(
        fs::read_to_string(root.join("dest.txt")).expect("read dest"),
        "winner-payload"
    );
    assert_no_staging(&root);
}

#[test]
fn copy_if_missing_has_one_concurrent_winner_and_no_partial_visibility() {
    let (root, context) = context_at("fs-copy-if-missing-race");
    let writers = 6_usize;
    let payloads: Vec<String> = (0..writers)
        .map(|index| {
            let marker = char::from(b'a' + index as u8);
            format!("writer-{index}:{}", marker.to_string().repeat(256 * 1024))
        })
        .collect();
    let known: std::collections::BTreeSet<String> = payloads.iter().cloned().collect();
    for (index, payload) in payloads.iter().enumerate() {
        fs::write(root.join(format!("source-{index}.txt")), payload).expect("source payload");
    }
    let context = Arc::new(context);
    let start = Arc::new(Barrier::new(writers + 1));
    let mut writer_handles = Vec::new();
    for index in 0..writers {
        let context = Arc::clone(&context);
        let start = Arc::clone(&start);
        writer_handles.push(thread::spawn(move || {
            start.wait();
            copy_if_missing_via_rhai_named(
                &context,
                &format!("source-{index}.txt"),
                "race/winner.txt",
                &format!("race/result-{index}.txt"),
            )
        }));
    }

    let writers_done = Arc::new(AtomicBool::new(false));
    let reader_root = root.clone();
    let reader_known = known.clone();
    let reader_done = Arc::clone(&writers_done);
    let reader = thread::spawn(move || {
        let destination = reader_root.join("race/winner.txt");
        let mut observed = 0_usize;
        loop {
            match fs::read_to_string(&destination) {
                Ok(contents) => {
                    assert!(
                        reader_known.contains(&contents),
                        "reader observed a partial or foreign payload of length {}",
                        contents.len()
                    );
                    observed += 1;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("reader failed: {error}"),
            }
            if reader_done.load(Ordering::SeqCst) {
                if let Ok(contents) = fs::read_to_string(&destination) {
                    assert!(
                        reader_known.contains(&contents),
                        "reader observed a partial winner after writers finished"
                    );
                    observed += 1;
                }
                break;
            }
            thread::yield_now();
        }
        observed
    });

    start.wait();
    let wins = writer_handles
        .into_iter()
        .map(|handle| handle.join().expect("writer join"))
        .filter(|won| *won)
        .count();
    writers_done.store(true, Ordering::SeqCst);
    let observed = reader.join().expect("reader join");
    assert!(observed >= 1, "reader never observed the published winner");
    assert_eq!(wins, 1, "exactly one concurrent copy_if_missing must win");
    let winner = fs::read_to_string(root.join("race/winner.txt")).expect("read winner");
    assert!(
        known.contains(&winner),
        "published winner must be a complete payload"
    );
    assert_no_staging(&root);
}

#[test]
fn rhai_surface_registry_describes_copy_if_missing_atomic_publication() {
    let surface = crate::surface::rhai_surface_json();
    let function = surface["functions"]
        .as_array()
        .expect("functions")
        .iter()
        .find(|function| function["module"] == "fs" && function["name"] == "copy_if_missing")
        .expect("copy_if_missing surface entry");
    let description = function["description"].as_str().expect("description");
    assert!(
        description.contains("Atomically") && description.contains("absent"),
        "surface catalog must describe create-if-absent publication: {description}"
    );
}

#[cfg(unix)]
fn create_fifo(path: &Path) {
    let status = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .expect("spawn mkfifo");
    assert!(
        status.success(),
        "mkfifo {} failed: {status}",
        path.display()
    );
}
