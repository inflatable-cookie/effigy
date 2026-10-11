use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

std::thread_local! {
    static FIXTURE_ROOT: RefCell<Option<tempfile::TempDir>> = const { RefCell::new(None) };
}

static TEMP_WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn temp_workspace(prefix: &str, name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let seq = TEMP_WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    // Match the reviewed unit fixture arrangement: inside the checkout's
    // authenticated parent, Cargo-excluded, but visible to graph/scan walks.
    // One exact TempDir owner per test thread also owns clones and Git remotes.
    let fixture_root = FIXTURE_ROOT.with(|owner| {
        let mut owner = owner.borrow_mut();
        if owner.is_none() {
            let checkout = fs::canonicalize(env!("CARGO_MANIFEST_DIR")).expect("checkout root");
            let parent = fs::canonicalize(checkout.join("crates")).expect("fixture parent");
            assert!(
                parent.starts_with(&checkout),
                "fixture parent escaped checkout"
            );
            *owner = Some(
                tempfile::Builder::new()
                    .prefix("effigy-parent-fixtures-cli-")
                    .tempdir_in(parent)
                    .expect("owned contained CLI fixture root"),
            );
        }
        owner
            .as_ref()
            .expect("fixture root owner")
            .path()
            .to_path_buf()
    });
    let root = fixture_root.join(format!("{prefix}-{name}-{ts}-{seq}"));
    fs::create_dir_all(&root).expect("mkdir workspace");
    root
}
