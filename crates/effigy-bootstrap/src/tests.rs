use super::{
    derive_repo_name, normalize_bootstrap_repo_url, resolve_bootstrap_request,
    resolve_child_destination, resolve_submodule_policy, submodule_policy_label,
    BootstrapDbSeedInput,
};
use effigy_manifest::ManifestBootstrapSubmodulesPolicy;
use std::path::{Path, PathBuf};

#[test]
fn derive_repo_name_supports_https_and_ssh_git_urls() {
    assert_eq!(
        derive_repo_name("https://github.com/inflatable-cookie/effigy.git"),
        Some("effigy".to_owned())
    );
    assert_eq!(
        derive_repo_name("git@github.com:inflatable-cookie/loophole.git"),
        Some("loophole".to_owned())
    );
    assert_eq!(
        derive_repo_name("ssh://git@github.com/inflatable-cookie/northstar.git"),
        Some("northstar".to_owned())
    );
}

#[test]
fn normalize_bootstrap_repo_url_rewrites_scp_style_ssh_remotes() {
    assert_eq!(
        normalize_bootstrap_repo_url("git@github.com:betterthanclay/effigy.git"),
        "ssh://git@github.com/betterthanclay/effigy.git"
    );
    assert_eq!(
        normalize_bootstrap_repo_url("https://github.com/betterthanclay/effigy.git"),
        "https://github.com/betterthanclay/effigy.git"
    );
}

#[test]
fn resolve_bootstrap_request_defaults_destination_under_cwd() {
    let cwd = Path::new("/tmp/dev");
    let resolved = resolve_bootstrap_request(
        cwd,
        "git@github.com:inflatable-cookie/effigy.git",
        None,
        None,
        &[],
        false,
        false,
    )
    .expect("resolve bootstrap");
    assert_eq!(resolved.repo_name, "effigy");
    assert_eq!(resolved.destination, cwd.join("effigy"));
    assert_eq!(resolved.destination_source, "cwd-default");
}

#[test]
fn resolve_bootstrap_request_honors_explicit_relative_path() {
    let cwd = Path::new("/tmp/dev");
    let resolved = resolve_bootstrap_request(
        cwd,
        "git@github.com:inflatable-cookie/effigy.git",
        Some(Path::new("./sandbox/effigy-checkout")),
        Some("main"),
        &[BootstrapDbSeedInput {
            target: Some("cbs".to_owned()),
            path: PathBuf::from("./dumps/latest.sql"),
        }],
        true,
        true,
    )
    .expect("resolve bootstrap");
    assert_eq!(
        resolved.destination,
        cwd.join(PathBuf::from("./sandbox/effigy-checkout"))
    );
    assert_eq!(resolved.destination_source, "explicit-path");
    assert_eq!(resolved.branch.as_deref(), Some("main"));
    assert_eq!(
        resolved.db_seeds,
        vec![BootstrapDbSeedInput {
            target: Some("cbs".to_owned()),
            path: cwd.join("dumps/latest.sql"),
        }]
    );
    assert!(resolved.fresh);
    assert!(resolved.start_requested);
}

#[test]
fn submodule_policy_label_matches_manifest_variants() {
    assert_eq!(
        submodule_policy_label(ManifestBootstrapSubmodulesPolicy::None),
        "none"
    );
    assert_eq!(
        submodule_policy_label(ManifestBootstrapSubmodulesPolicy::Init),
        "init"
    );
    assert_eq!(
        submodule_policy_label(ManifestBootstrapSubmodulesPolicy::Recursive),
        "recursive"
    );
}

#[test]
fn resolve_submodule_policy_defaults_to_recursive_when_gitmodules_exists() {
    let root = std::env::temp_dir().join(format!("effigy-submodule-policy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir root");
    std::fs::write(root.join(".gitmodules"), "").expect("write gitmodules");

    assert_eq!(
        resolve_submodule_policy(&root, None),
        ManifestBootstrapSubmodulesPolicy::Recursive
    );
    assert_eq!(
        resolve_submodule_policy(&root, Some(ManifestBootstrapSubmodulesPolicy::None)),
        ManifestBootstrapSubmodulesPolicy::None
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn resolve_child_destination_allows_sibling_paths_under_root_parent() {
    let root = Path::new("/tmp/dev/effigy");
    let resolved = resolve_child_destination(root, "../dev-platform").expect("resolve child");
    assert_eq!(resolved, Path::new("/tmp/dev/dev-platform"));
}

#[test]
fn resolve_child_destination_rejects_escape_above_root_parent() {
    let root = Path::new("/tmp/dev/effigy");
    let error = resolve_child_destination(root, "../../outside").expect_err("should reject");
    assert!(error
        .to_string()
        .contains("cannot escape the root repo parent directory"));
}
