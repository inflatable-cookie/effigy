//! One collected prerequisite report for `effigy bootstrap` (Q-009 item 6).
//!
//! Bootstrap fails early with a short, actionable list instead of one
//! per-phase failure at a time. It only reports what it can observe before it
//! mutates anything: the tools it will invoke itself, the toolchains the
//! checkout obviously needs, and the user-owned gateway when the project
//! declares routed domains.
//!
//! It never sets up the gateway, never starts a runtime, never authenticates a
//! login and never installs a toolchain.

use std::path::Path;

use effigy_core::path_probe::command_available_in_path;
use effigy_manifest::{ManifestJsPackageManager, TaskManifest};

/// One missing prerequisite with the action that resolves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct MissingPrerequisite {
    pub missing: String,
    pub fix: String,
}

impl MissingPrerequisite {
    fn new(missing: impl Into<String>, fix: impl Into<String>) -> Self {
        Self {
            missing: missing.into(),
            fix: fix.into(),
        }
    }
}

/// Checks that need no manifest: bootstrap itself invokes these tools.
pub(in crate::runner) fn collect_machine() -> Vec<MissingPrerequisite> {
    let mut missing = Vec::new();
    if !command_available_in_path("git") {
        missing.push(MissingPrerequisite::new(
            "`git` is not on PATH",
            "install git; bootstrap clones the checkout, its pinned `[sources.<name>]` and its submodules with it",
        ));
    }
    match std::env::var_os("HOME").map(std::path::PathBuf::from) {
        Some(home) if home.is_absolute() => {}
        _ => missing.push(MissingPrerequisite::new(
            "HOME is not an absolute directory",
            "set HOME to an absolute directory; source preparation and Effigy's state root need it",
        )),
    }
    missing
}

/// Checks that need the checkout's manifest: the tools its shape implies.
pub(in crate::runner) fn collect_checkout(
    checkout: &Path,
    manifest: &TaskManifest,
) -> Vec<MissingPrerequisite> {
    let mut missing = Vec::new();

    if manifest.containers.is_some() && !container_runtime_available() {
        missing.push(MissingPrerequisite::new(
            "no container runtime is available (neither `docker` nor `colima` is on PATH)",
            "install Docker Desktop or Colima and start it, or pin the backend you use with `effigy config set containers.backend <containerd|docker>`",
        ));
    }

    if checkout.join("package.json").is_file() {
        if !command_available_in_path("node") {
            missing.push(MissingPrerequisite::new(
                "`node` is not on PATH",
                "install Node.js; this checkout has a `package.json`",
            ));
        }
        let manager = manifest
            .package_manager
            .as_ref()
            .and_then(|config| config.js)
            .unwrap_or(ManifestJsPackageManager::Bun);
        if let Some(binary) = manager.binary_name() {
            if !command_available_in_path(binary) {
                missing.push(MissingPrerequisite::new(
                    format!("`{binary}` is not on PATH (the declared JavaScript package manager)"),
                    format!(
                        "install {binary}, or declare the package manager this checkout uses under `[package_manager] js`"
                    ),
                ));
            }
        }
    }

    if checkout.join("Cargo.toml").is_file() || checkout.join("rust-toolchain.toml").is_file() {
        for tool in ["cargo", "rustc"] {
            if !command_available_in_path(tool) {
                missing.push(MissingPrerequisite::new(
                    format!("`{tool}` is not on PATH (this is a Rust checkout)"),
                    "install the Rust toolchain (`rustup`), then re-run",
                ));
            }
        }
    }

    if let Some(gateway) = missing_gateway(manifest) {
        missing.push(gateway);
    }

    missing
}

/// The gateway is user-owned: bootstrap reports it, and never starts it.
fn missing_gateway(manifest: &TaskManifest) -> Option<MissingPrerequisite> {
    let declares_routes = manifest.containers.as_ref().is_some_and(|containers| {
        containers.environments.values().any(|container| {
            container
                .dns
                .as_ref()
                .is_some_and(|dns| !dns.resolved_routes().is_empty())
        })
    });
    if !declares_routes {
        return None;
    }
    let gateway_dir = match crate::runner::gateway_command::gateway_dir() {
        Ok(dir) => dir,
        Err(error) => {
            return Some(MissingPrerequisite::new(
                format!("the user-owned gateway state is unreadable: {error}"),
                "fix the gateway state directory, then start the gateway yourself with `effigy gateway up`",
            ))
        }
    };
    let config = effigy_gateway::server::GatewayConfig::standard(gateway_dir);
    let diagnosis = effigy_gateway::server::diagnose_gateway_status(&config);
    let running = matches!(
        diagnosis.record,
        effigy_gateway::server::GatewayRecordKind::Authenticated
            | effigy_gateway::server::GatewayRecordKind::LegacyPidOnly
    );
    if running {
        return None;
    }
    Some(MissingPrerequisite::new(
        format!(
            "the user-owned gateway is not running ({}), but this project declares routed domains",
            diagnosis.record.label()
        ),
        "start it yourself with `effigy gateway up`; bootstrap never starts, configures or logs in for you",
    ))
}

fn container_runtime_available() -> bool {
    command_available_in_path("docker") || command_available_in_path("colima")
}

/// Render one actionable list. A short report, not a wall of detail.
pub(in crate::runner) fn render(missing: &[MissingPrerequisite]) -> String {
    let count = missing.len();
    let mut message = format!(
        "bootstrap cannot start: {count} prerequisite{} missing\n",
        if count == 1 { " is" } else { "s are" }
    );
    for entry in missing {
        message.push_str(&format!("- {}\n  fix: {}\n", entry.missing, entry.fix));
    }
    message.push_str(
        "Nothing was started, configured or logged in for you. See `docs/guides/057-bootstrap-repo-bringup.md` and CHANGELOG.md.",
    );
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_test_support::{lock_test, EnvGuard};
    use effigy_manifest::load_task_manifest;

    fn fixture_root(name: &str) -> effigy_test_support::Fixture {
        effigy_test_support::Fixture::workspace(name).expect("private fixture root")
    }

    /// A PATH holding exactly the named tools, so host tools never leak in.
    fn fake_bin(root: &Path, tools: &[&str]) -> String {
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir bin");
        for tool in tools {
            let path = bin.join(tool);
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("write tool");
            let mut perms = std::fs::metadata(&path).expect("stat tool").permissions();
            {
                use std::os::unix::fs::PermissionsExt;
                perms.set_mode(0o755);
            }
            std::fs::set_permissions(&path, perms).expect("chmod tool");
        }
        bin.display().to_string()
    }

    fn manifest(root: &Path, body: &str) -> TaskManifest {
        std::fs::write(root.join("effigy.toml"), body).expect("write manifest");
        load_task_manifest(&root.join("effigy.toml")).expect("load manifest")
    }

    #[test]
    fn missing_git_and_home_are_collected_together() {
        let _guard = lock_test();
        let root = fixture_root("bootstrap-prereqs-machine-");
        let path = fake_bin(root.path(), &[]);
        let _env = EnvGuard::set_many(&[
            ("PATH", Some(path)),
            ("HOME", Some("relative/home".to_owned())),
        ]);
        let missing = collect_machine();
        let rendered = render(&missing);
        assert!(rendered.contains("`git` is not on PATH"), "{rendered}");
        assert!(rendered.contains("HOME is not an absolute"), "{rendered}");
        assert!(rendered.contains("prerequisites are missing"), "{rendered}");
    }

    #[test]
    fn a_rust_checkout_reports_its_toolchain_and_a_js_checkout_its_manager() {
        let _guard = lock_test();
        let root = fixture_root("bootstrap-prereqs-toolchains-");
        let path = fake_bin(root.path(), &[]);
        let _env = EnvGuard::set_many(&[("PATH", Some(path))]);
        std::fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"x\"\n")
            .expect("write cargo manifest");
        std::fs::write(root.path().join("package.json"), "{}\n").expect("write package json");
        let manifest = manifest(root.path(), "[package_manager]\njs = \"pnpm\"\n");

        let missing = collect_checkout(root.path(), &manifest);
        let rendered = render(&missing);
        assert!(rendered.contains("`cargo` is not on PATH"), "{rendered}");
        assert!(rendered.contains("`rustc` is not on PATH"), "{rendered}");
        assert!(rendered.contains("`pnpm` is not on PATH"), "{rendered}");
        assert!(rendered.contains("`node` is not on PATH"), "{rendered}");
    }

    #[test]
    fn a_declared_container_environment_requires_a_runtime() {
        let _guard = lock_test();
        let root = fixture_root("bootstrap-prereqs-runtime-");
        let path = fake_bin(root.path(), &["git", "node", "bun"]);
        let _env = EnvGuard::set_many(&[("PATH", Some(path))]);
        std::fs::write(root.path().join("package.json"), "{}\n").expect("write package json");
        let manifest = manifest(
            root.path(),
            "[containers]\ndefault = \"dev\"\n\n[containers.dev]\nprimary_service = \"app\"\n",
        );

        let missing = collect_checkout(root.path(), &manifest);
        let rendered = render(&missing);
        assert!(
            rendered.contains("no container runtime is available"),
            "{rendered}"
        );
        assert!(
            rendered.contains("effigy config set containers.backend"),
            "{rendered}"
        );
    }

    #[test]
    fn a_project_without_routed_domains_does_not_require_the_gateway() {
        let _guard = lock_test();
        let root = fixture_root("bootstrap-prereqs-gateway-");
        let path = fake_bin(root.path(), &["git"]);
        let _env = EnvGuard::set_many(&[("PATH", Some(path))]);
        let manifest = manifest(
            root.path(),
            "[containers]\ndefault = \"dev\"\n\n[containers.dev]\nprimary_service = \"app\"\n\n[tasks.dev]\nrun = \"true\"\n",
        );
        // No runtime either, but the gateway must not be reported.
        let missing = collect_checkout(root.path(), &manifest);
        assert!(!render(&missing).contains("gateway"), "{missing:?}");
    }

    #[test]
    fn a_ready_environment_reports_nothing() {
        let _guard = lock_test();
        let root = fixture_root("bootstrap-prereqs-clean-");
        let path = fake_bin(
            root.path(),
            &["git", "node", "bun", "cargo", "rustc", "docker"],
        );
        let _env = EnvGuard::set_many(&[("PATH", Some(path))]);
        std::fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"x\"\n")
            .expect("write cargo manifest");
        std::fs::write(root.path().join("package.json"), "{}\n").expect("write package json");
        let manifest = manifest(
            root.path(),
            "[package_manager]\njs = \"bun\"\n\n[containers]\ndefault = \"dev\"\n\n[containers.dev]\nprimary_service = \"app\"\n",
        );

        assert_eq!(collect_machine(), Vec::new());
        assert_eq!(collect_checkout(root.path(), &manifest), Vec::new());
    }
}
