use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

std::thread_local! {
    static FIXTURE_ROOT: RefCell<Option<effigy_test_support::Fixture>> = const { RefCell::new(None) };
}

static TEMP_WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn temp_workspace(prefix: &str, name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let seq = TEMP_WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    // The thread owns all nested workspaces, clones and remotes. Authenticated
    // nested selectors use the central in-repository exception (contract 049).
    let fixture_root = FIXTURE_ROOT.with(|owner| {
        let mut owner = owner.borrow_mut();
        if owner.is_none() {
            *owner = Some(
                effigy_test_support::Fixture::workspace("effigy-parent-fixtures-cli-")
                    .expect("owned CLI fixture root"),
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
