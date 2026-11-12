//! Machine-local bundle inputs (`~/.effigy/config.toml`
//! `[bundle.<name>].inputs`) and the precedence ladder they sit in.
//!
//! One test fixture shape: a path bundle that declares a `php_version` string
//! input (default `8.4`) and renders it into a task command, a project manifest
//! that selects that bundle, and a scratch Effigy home holding the machine-local
//! file. Every test owns its temporary tree; the real `~/.effigy` is never read.

use std::path::{Path, PathBuf};

use effigy_manifest::{
    load_task_manifest_with_inspection, with_test_user_config_home, BundleInputSource,
    LoadedTaskManifest, ManifestManagedRun, RetiredNoticeDelivery,
};

/// Serialise the tests that read the process-wide notice ledger.
static NOTICE_LEDGER: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Serialise the tests that toggle the process-wide local-overlay switch.
static LOCAL_OVERLAY: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Fixture {
    owner: tempfile::TempDir,
}

impl Fixture {
    /// A repo with a path bundle declaring one `php_version` input and a
    /// scratch Effigy home at `<owner>/home/.effigy`.
    fn new(bundle_name: &str) -> Self {
        let owner = tempfile::tempdir().expect("private fixture root");
        let repo = owner.path().join("repo");
        let bundle = repo.join("bundles/acme");
        std::fs::create_dir_all(&bundle).expect("mkdir bundle");
        std::fs::write(
            bundle.join("bundle.toml"),
            format!(
                r#"
[bundle]
name = "{bundle_name}"
description = "Bundle input fixture."

[[inputs]]
name = "php_version"
type = "string"
default = "8.4"
description = "PHP version to run."
"#
            ),
        )
        .expect("write descriptor");
        std::fs::write(
            bundle.join("effigy.toml"),
            "[tasks.php]\nrun = \"echo php {{ inputs.php_version }}\"\n",
        )
        .expect("write defaults");
        std::fs::write(
            repo.join("effigy.toml"),
            "[bundle]\nbase = { type = \"path\", dir = \"bundles/acme\" }\n",
        )
        .expect("write manifest");
        std::fs::create_dir_all(Self::effigy_home(owner.path())).expect("mkdir home");
        Self { owner }
    }

    fn effigy_home(owner: &Path) -> PathBuf {
        owner.join("home/.effigy")
    }

    fn repo(&self) -> PathBuf {
        self.owner.path().join("repo")
    }

    fn manifest_path(&self) -> PathBuf {
        self.repo().join("effigy.toml")
    }

    /// Write the machine-local config file.
    fn write_user_config(&self, body: &str) {
        std::fs::write(
            Self::effigy_home(self.owner.path()).join("config.toml"),
            body,
        )
        .expect("write user config");
    }

    /// Append one `[bundle]` key to the project manifest.
    fn append_project_bundle_key(&self, key: &str, value: &str) {
        let source = std::fs::read_to_string(self.manifest_path()).expect("read manifest");
        std::fs::write(self.manifest_path(), format!("{source}{key} = {value}\n"))
            .expect("write manifest");
    }

    /// Repoint the project at a stale pin: `.effigy/sources.lock.json` records
    /// a snapshot under `cache/sources/pinned` whose descriptor declares no
    /// `php_version`, while the project's own `[bundle]` declaration has moved
    /// on to `bundles/acme` (which does declare it). That is the shape of a
    /// checkout whose pin is behind its declaration.
    fn write_stale_pin(&self, name: &str) {
        let object = self.repo().join(".effigy/cache/sources/pinned");
        std::fs::create_dir_all(&object).expect("mkdir pinned snapshot");
        std::fs::write(
            object.join("bundle.toml"),
            format!(
                r#"
[bundle]
name = "{name}"
description = "Pinned bundle."

[[inputs]]
name = "node_version"
type = "string"
default = "22"
description = "Node version to run."
"#
            ),
        )
        .expect("write pinned descriptor");
        std::fs::write(
            object.join("effigy.toml"),
            "[tasks.php]\nrun = \"echo php pinned\"\n",
        )
        .expect("write pinned defaults");
        std::fs::write(
            self.repo().join(".effigy/sources.lock.json"),
            r#"{"version":1,"sources":{"bundle":{"declaration":{"type":"path","dir":"bundles/pinned"},"object":"cache/sources/pinned","version":"9b1f7a4c2d6e8f0a1b3c5d7e9f0a2b4c6d8e0f1a3b5c7d9e0f2a4b6c8d0e1f3a"}}}"#,
        )
        .expect("write source lock");
    }

    /// Load the project manifest with this fixture's Effigy home active.
    fn load(&self) -> LoadedTaskManifest {
        with_test_user_config_home(&Self::effigy_home(self.owner.path()), || {
            load_task_manifest_with_inspection(&self.manifest_path()).expect("load manifest")
        })
    }

    /// Load and expect the bundle load to refuse.
    fn load_error(&self) -> String {
        with_test_user_config_home(&Self::effigy_home(self.owner.path()), || {
            load_task_manifest_with_inspection(&self.manifest_path())
                .expect_err("bundle load should refuse")
                .to_string()
        })
    }
}

/// The rendered value the loaded manifest's `php` task would run.
fn rendered_php(loaded: &LoadedTaskManifest) -> String {
    let task = loaded.manifest.tasks.get("php").expect("php task");
    let ManifestManagedRun::Command(command) = task.run.as_ref().expect("run") else {
        panic!("php task should carry a command");
    };
    command.trim().to_owned()
}

/// The effective `php_version` resolution the manifest reports.
fn php_resolution(loaded: &LoadedTaskManifest) -> (String, BundleInputSource) {
    let input = loaded
        .bundle_inputs
        .iter()
        .find(|input| input.name == "php_version")
        .expect("php_version resolution");
    (
        input.value.as_str().expect("string input").to_owned(),
        input.source,
    )
}

#[test]
fn a_bundle_default_applies_with_no_user_config() {
    let fixture = Fixture::new("acme");
    let loaded = fixture.load();
    assert_eq!(rendered_php(&loaded), "echo php 8.4");
    assert_eq!(
        php_resolution(&loaded),
        ("8.4".to_owned(), BundleInputSource::Default)
    );
    assert_eq!(loaded.bundle_name.as_deref(), Some("acme"));
}

#[test]
fn a_user_global_input_overrides_the_bundle_default() {
    let fixture = Fixture::new("acme");
    fixture.write_user_config(
        r#"
[bundle.acme]
inputs = { php_version = "8.5" }
"#,
    );
    let loaded = fixture.load();
    assert_eq!(rendered_php(&loaded), "echo php 8.5");
    assert_eq!(
        php_resolution(&loaded),
        ("8.5".to_owned(), BundleInputSource::UserConfig)
    );
}

#[test]
fn a_project_value_beats_the_user_global_input() {
    let fixture = Fixture::new("acme");
    fixture.write_user_config(
        r#"
[bundle.acme]
inputs = { php_version = "8.5" }
"#,
    );
    fixture.append_project_bundle_key("php_version", "\"8.6\"");
    let loaded = fixture.load();
    assert_eq!(rendered_php(&loaded), "echo php 8.6");
    assert_eq!(
        php_resolution(&loaded),
        ("8.6".to_owned(), BundleInputSource::Manifest)
    );
}

#[test]
fn a_local_overlay_value_beats_both_the_repo_and_the_user_global_input() {
    let _guard = LOCAL_OVERLAY.lock().expect("overlay lock");
    std::env::remove_var("EFFIGY_NO_LOCAL_OVERLAY");
    let fixture = Fixture::new("acme");
    fixture.write_user_config(
        r#"
[bundle.acme]
inputs = { php_version = "8.5" }
"#,
    );
    fixture.append_project_bundle_key("php_version", "\"8.6\"");
    std::fs::write(
        fixture.repo().join("effigy.local.toml"),
        "[bundle]\nphp_version = \"8.7\"\n",
    )
    .expect("write local overlay");
    let loaded = fixture.load();
    assert_eq!(rendered_php(&loaded), "echo php 8.7");
    assert_eq!(
        php_resolution(&loaded),
        ("8.7".to_owned(), BundleInputSource::Manifest)
    );
}

#[test]
fn a_block_for_another_bundle_does_nothing() {
    let fixture = Fixture::new("acme");
    fixture.write_user_config(
        r#"
[bundle.other]
inputs = { php_version = "9.9", undeclared = true }
"#,
    );
    let loaded = fixture.load();
    assert_eq!(rendered_php(&loaded), "echo php 8.4");
    assert_eq!(
        php_resolution(&loaded),
        ("8.4".to_owned(), BundleInputSource::Default)
    );
}

#[test]
fn a_hand_written_underscore_key_matches_a_hyphenated_bundle_name() {
    let fixture = Fixture::new("acme-bundle");
    fixture.write_user_config(
        r#"
[bundle.acme_bundle]
inputs = { php_version = "8.5" }
"#,
    );
    let loaded = fixture.load();
    assert_eq!(loaded.bundle_name.as_deref(), Some("acme-bundle"));
    assert_eq!(rendered_php(&loaded), "echo php 8.5");
}

/// Load with the notice ledger collecting into this test's own view, and return
/// the warnings this load produced.
fn load_and_collect_warnings(fixture: &Fixture) -> (LoadedTaskManifest, Vec<String>) {
    effigy_manifest::set_retired_notice_delivery(RetiredNoticeDelivery::JsonEnvelope);
    let _ = effigy_manifest::take_retired_notices();
    let loaded = fixture.load();
    let warnings = effigy_manifest::take_retired_notices();
    (loaded, warnings)
}

#[test]
fn an_undeclared_user_input_warns_and_is_ignored() {
    let _guard = NOTICE_LEDGER.lock().expect("notice ledger lock");
    let fixture = Fixture::new("undeclared-acme");
    fixture.write_user_config(
        r#"
[bundle.undeclared-acme]
inputs = { php_versoin = "8.5" }
"#,
    );
    let (loaded, warnings) = load_and_collect_warnings(&fixture);
    assert_eq!(rendered_php(&loaded), "echo php 8.4");
    assert_eq!(
        php_resolution(&loaded),
        ("8.4".to_owned(), BundleInputSource::Default)
    );
    let warning = warnings
        .iter()
        .find(|warning| warning.contains("php_versoin"))
        .unwrap_or_else(|| panic!("no warning named the undeclared input: {warnings:?}"));
    assert!(warning.contains("config.toml"), "{warning}");
    assert!(warning.contains("[bundle.undeclared-acme]"), "{warning}");
    assert!(warning.contains("ignoring"), "{warning}");
}

#[test]
fn an_ill_typed_user_input_warns_and_is_ignored() {
    let _guard = NOTICE_LEDGER.lock().expect("notice ledger lock");
    let fixture = Fixture::new("ill-typed-acme");
    fixture.write_user_config(
        r#"
[bundle.ill-typed-acme]
inputs = { php_version = 8.5 }
"#,
    );
    let (loaded, warnings) = load_and_collect_warnings(&fixture);
    assert_eq!(rendered_php(&loaded), "echo php 8.4");
    assert_eq!(
        php_resolution(&loaded),
        ("8.4".to_owned(), BundleInputSource::Default)
    );
    let warning = warnings
        .iter()
        .find(|warning| warning.contains("8.5"))
        .unwrap_or_else(|| panic!("no warning named the ill-typed value: {warnings:?}"));
    assert!(warning.contains("config.toml"), "{warning}");
    assert!(warning.contains("php_version"), "{warning}");
    assert!(warning.contains("a string"), "{warning}");
}

#[test]
fn an_ill_typed_user_input_falls_back_to_the_project_value() {
    let _guard = NOTICE_LEDGER.lock().expect("notice ledger lock");
    let fixture = Fixture::new("ill-typed-project-acme");
    fixture.write_user_config(
        r#"
[bundle.ill-typed-project-acme]
inputs = { php_version = true }
"#,
    );
    fixture.append_project_bundle_key("php_version", "\"8.6\"");
    let (loaded, _warnings) = load_and_collect_warnings(&fixture);
    assert_eq!(rendered_php(&loaded), "echo php 8.6");
    assert_eq!(
        php_resolution(&loaded),
        ("8.6".to_owned(), BundleInputSource::Manifest)
    );
}

#[test]
fn a_malformed_user_config_warns_instead_of_failing_every_project() {
    let _guard = NOTICE_LEDGER.lock().expect("notice ledger lock");
    let fixture = Fixture::new("malformed-acme");
    fixture.write_user_config("this is not valid = toml = at all =");
    let (loaded, warnings) = load_and_collect_warnings(&fixture);
    assert_eq!(rendered_php(&loaded), "echo php 8.4");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("config.toml") && warning.contains("ignoring")),
        "{warnings:?}"
    );
}

#[test]
fn a_stale_pin_does_not_warn_about_the_inputs_it_is_about_to_replace() {
    let _guard = NOTICE_LEDGER.lock().expect("notice ledger lock");
    let fixture = Fixture::new("stale-acme");
    fixture.write_user_config(
        r#"
[bundle.stale-acme]
inputs = { php_version = "8.5" }
"#,
    );
    fixture.write_stale_pin("stale-acme");
    let (loaded, warnings) = load_and_collect_warnings(&fixture);
    // The pinned snapshot is what renders, and the value it cannot place is
    // still ignored rather than used.
    assert_eq!(rendered_php(&loaded), "echo php pinned");
    assert_eq!(loaded.bundle_name.as_deref(), Some("stale-acme"));
    let node = loaded
        .bundle_inputs
        .iter()
        .find(|input| input.name == "node_version")
        .expect("the pinned descriptor's input resolves");
    assert_eq!(node.value.as_str(), Some("22"));
    // A snapshot the next Prepare path replaces is not the effective bundle:
    // judging the operator's file against it claimed the bundle did not
    // declare an input the refreshed pin does declare (papercut 1f529edc).
    assert!(
        !warnings
            .iter()
            .any(|warning| warning.contains("php_version")),
        "no warning judges the file against the stale pin: {warnings:?}"
    );
}

#[test]
fn a_project_input_the_bundle_does_not_declare_still_refuses() {
    let fixture = Fixture::new("strict-acme");
    fixture.append_project_bundle_key("typo", "\"nope\"");
    let error = fixture.load_error();
    assert!(
        error.contains("local bundle `strict-acme` does not declare input `typo`"),
        "{error}"
    );
}
