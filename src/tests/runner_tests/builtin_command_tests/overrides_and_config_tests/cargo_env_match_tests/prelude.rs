pub(super) use super::super::prelude::*;

pub(super) const CARGO_ENV_PROBE_SCRIPT: &str = "#!/bin/sh\nprintf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n";

pub(super) fn assert_cargo_env_applied(root: &PathBuf, expected_home: &str, expected_target: &str) {
    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root"]);
    assert_cargo_env_matches(&root.join("cargo-env.log"), expected_home, expected_target);
}
