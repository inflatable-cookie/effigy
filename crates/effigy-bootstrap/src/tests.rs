use super::{
    derive_repo_name, normalize_bootstrap_repo_url, plan_bootstrap_request, render_bootstrap_plan,
    render_bootstrap_result, resolve_bootstrap_request, BootstrapPlan, BootstrapProgressEvent,
    BootstrapRequest, PinnedSourceReport, SubmoduleReport,
};
use effigy_manifest::PinnedSourceState;
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
    let resolved =
        resolve_bootstrap_request(cwd, Some("git@github.com:acme/app.git"), None, None, true)
            .expect("resolve request");
    assert_eq!(resolved.destination, cwd.join("app"));
    assert_eq!(resolved.destination_source, "cwd-default");
    assert_eq!(
        resolved.repo_url.as_deref(),
        Some("git@github.com:acme/app.git")
    );
    assert!(resolved.start_requested);
}

#[test]
fn resolve_bootstrap_request_uses_an_existing_checkout_without_a_url() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let resolved = resolve_bootstrap_request(fixture.path(), None, None, None, false)
        .expect("resolve request");
    assert_eq!(resolved.repo_url, None);
    assert_eq!(resolved.destination, fixture.path());
    assert_eq!(resolved.destination_source, "existing-checkout");
    assert!(!resolved.start_requested);
}

#[test]
fn resolve_bootstrap_request_refuses_a_branch_without_a_url() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let error = resolve_bootstrap_request(fixture.path(), None, None, Some("main"), true)
        .expect_err("branch without url");
    assert!(error.to_string().contains("`--branch`"), "{error}");
}

#[test]
fn resolve_bootstrap_request_requires_an_existing_directory_without_a_url() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let error =
        resolve_bootstrap_request(fixture.path(), None, Some(Path::new("missing")), None, true)
            .expect_err("missing checkout");
    assert!(error.to_string().contains("not a directory"), "{error}");
}

#[test]
fn a_plan_lists_clone_sources_submodules_setup_and_start() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let checkout = fixture.path().join("app");
    std::fs::create_dir_all(&checkout).expect("mkdir");
    std::fs::write(
        checkout.join("effigy.toml"),
        r#"
[bootstrap]
setup = ["workspace:js:prepare", "db:seed"]
start = "dev"
submodules = true
"#,
    )
    .expect("write manifest");
    let request = resolve_bootstrap_request(fixture.path(), None, Some(&checkout), None, true)
        .expect("request");
    let plan = plan_bootstrap_request(&request).expect("plan");
    assert!(plan.manifest_found);
    assert_eq!(plan.setup, vec!["workspace:js:prepare", "db:seed"]);
    assert_eq!(plan.start.as_deref(), Some("dev"));
    assert!(plan.submodules);

    let text = render_bootstrap_plan(&plan, false);
    assert!(text.contains("clone: no"), "{text}");
    assert!(text.contains("sources: none"), "{text}");
    assert!(text.contains("submodules: recursive init"), "{text}");
    assert!(
        text.contains("setup: workspace:js:prepare, db:seed"),
        "{text}"
    );
    assert!(text.contains("start: dev"), "{text}");

    let json: serde_json::Value =
        serde_json::from_str(&render_bootstrap_plan(&plan, true)).expect("plan json");
    assert_eq!(json["schema"], "effigy.bootstrap.v1");
    assert_eq!(json["plan"]["setup"][0], "workspace:js:prepare");
    assert_eq!(json["plan"]["start"], "dev");
    assert_eq!(json["plan"]["submodules"], true);
}

#[test]
fn a_plan_reports_an_unreadable_manifest_without_failing() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let checkout = fixture.path().join("app");
    std::fs::create_dir_all(&checkout).expect("mkdir");
    std::fs::write(checkout.join("effigy.toml"), "[bootstrap]\nsetup = 5\n")
        .expect("write manifest");
    let request = resolve_bootstrap_request(fixture.path(), None, Some(&checkout), None, true)
        .expect("request");
    let plan = plan_bootstrap_request(&request).expect("plan");
    assert!(plan.manifest_found);
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    let reason = plan.manifest_unreadable.as_deref().unwrap_or_default();
    assert!(reason.contains("bootstrap"), "{reason}");
    let text = render_bootstrap_plan(&plan, false);
    assert!(text.contains("manifest: unreadable"), "{text}");
}

#[test]
fn a_plan_reports_retired_bootstrap_refusals_with_the_migration() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let checkout = fixture.path().join("app");
    std::fs::create_dir_all(&checkout).expect("mkdir");
    std::fs::write(
        checkout.join("effigy.toml"),
        "[bootstrap]\nstart = \"dev\"\n\n[[bootstrap.children]]\npath = \"../book\"\nrepo = \"git@example.test:book.git\"\n",
    )
    .expect("write manifest");
    let request = resolve_bootstrap_request(fixture.path(), None, Some(&checkout), None, true)
        .expect("request");
    let plan = plan_bootstrap_request(&request).expect("plan");
    assert!(
        plan.manifest_unreadable.is_none(),
        "{:?}",
        plan.manifest_unreadable
    );
    assert_eq!(plan.refusals.len(), 1, "{:?}", plan.refusals);
    assert!(plan.refusals[0].contains("[bootstrap].children"));
    assert!(plan.refusals[0].contains("[sources.<name>]"));
    let text = render_bootstrap_plan(&plan, false);
    assert!(text.contains("refused:"), "{text}");
    assert!(text.contains("[sources.<name>]"), "{text}");
    let json: serde_json::Value =
        serde_json::from_str(&render_bootstrap_plan(&plan, true)).expect("plan json");
    assert_eq!(json["refusals"].as_array().map(Vec::len), Some(1));
}

fn source_report(state: PinnedSourceState, dirty: bool) -> PinnedSourceReport {
    PinnedSourceReport {
        name: "book".to_owned(),
        git: "git@example.test:book.git".to_owned(),
        revision: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        path: PathBuf::from("/dev/book"),
        state,
        head: Some("89abcdef0123456789abcdef0123456789abcdef".to_owned()),
        dirty,
        detail: Some("details".to_owned()),
    }
}

#[test]
fn a_result_renders_sources_submodules_setup_and_start() {
    let result = super::BootstrapExecutionResult {
        request: BootstrapRequest {
            repo_url: Some("git@example.test:app.git".to_owned()),
            destination: PathBuf::from("/dev/app"),
            destination_source: "explicit-path",
            branch: None,
            start_requested: true,
        },
        root_repo_state: "cloned",
        manifest_path: PathBuf::from("/dev/app/effigy.toml"),
        manifest_found: true,
        bootstrap_contract_found: true,
        sources: vec![source_report(PinnedSourceState::Drift, true)],
        submodules: SubmoduleReport {
            requested: true,
            applied: false,
            drift: vec!["submodule `vendor/lib` has local changes; left untouched".to_owned()],
        },
        setup_selectors: vec!["prepare".to_owned()],
        setup_ran: vec!["prepare".to_owned()],
        start_selector: Some("dev".to_owned()),
        start_ran: true,
        warnings: vec!["pinned source `book` is drift".to_owned()],
    };
    let text = render_bootstrap_result(&result, false);
    assert!(text.contains("root repo: cloned"), "{text}");
    assert!(text.contains("- source book: drift"), "{text}");
    assert!(text.contains("submodules: recursive init"), "{text}");
    assert!(text.contains("[warn] submodule `vendor/lib`"), "{text}");
    assert!(text.contains("setup: prepare"), "{text}");
    assert!(text.contains("start: dev (ran)"), "{text}");

    let json: serde_json::Value =
        serde_json::from_str(&render_bootstrap_result(&result, true)).expect("result json");
    assert_eq!(json["schema"], "effigy.bootstrap.v1");
    assert_eq!(json["sources"][0]["state"], "drift");
    assert_eq!(json["submodules"]["requested"], true);
    assert_eq!(json["setup"]["ran"][0], "prepare");
    assert_eq!(json["start"]["ran"], true);
}

#[test]
fn a_result_without_a_start_says_so() {
    let result = super::BootstrapExecutionResult {
        request: BootstrapRequest {
            repo_url: None,
            destination: PathBuf::from("/dev/app"),
            destination_source: "existing-checkout",
            branch: None,
            start_requested: true,
        },
        root_repo_state: "existing",
        manifest_path: PathBuf::from("/dev/app/effigy.toml"),
        manifest_found: true,
        bootstrap_contract_found: false,
        sources: Vec::new(),
        submodules: SubmoduleReport::default(),
        setup_selectors: Vec::new(),
        setup_ran: Vec::new(),
        start_selector: None,
        start_ran: false,
        warnings: Vec::new(),
    };
    let text = render_bootstrap_result(&result, false);
    assert!(text.contains("no [bootstrap] contract was found"), "{text}");
    assert!(text.contains("setup: none"), "{text}");
    assert!(text.contains("start: none"), "{text}");
}

#[test]
fn plan_progress_variants_are_constructible() {
    let _ = BootstrapPlan {
        request: BootstrapRequest {
            repo_url: None,
            destination: Path::new("/dev/app").to_path_buf(),
            destination_source: "existing-checkout",
            branch: None,
            start_requested: false,
        },
        manifest_found: false,
        manifest_unreadable: None,
        refusals: Vec::new(),
        sources: Vec::new(),
        submodules: false,
        setup: Vec::new(),
        start: None,
    };
    let _ = BootstrapProgressEvent::SourcesPrepared {
        destination: Path::new("/dev/app").to_path_buf(),
        cloned: 0,
        drift: 0,
    };
}
