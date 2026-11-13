//! Fail-fast JavaScript dependency drift before managed host processes start.
//!
//! A stale install used to surface only as the child's own runtime failure:
//! the supervisor started every process, the front end answered `500`, and
//! only then did the start abort. The preflight compares the installed copy of
//! each *declared* dependency against the checkout's own lock resolution and
//! refuses the drift here instead, naming the repository's dependency
//! preparation.
//!
//! The check is driven by the manifest's declared `[package_manager].js` and a
//! lockfile this build can read. It is never a universal `node_modules` rule: a
//! checkout with no declaration, a lock format this build cannot read, a
//! machine-local `effigy deps link` package, and a declared dependency with no
//! installed copy at all are left alone. Only an installed copy that exists and
//! disagrees with the lock is proof of drift.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use effigy_containers::EffectiveContainerPolicy;
use effigy_manifest::{ManifestJsPackageManager, TaskManifest};

use super::configuration::resolve_host_process_cwd;
use crate::runner::error::RunnerError;

const MANIFEST_FILE: &str = "effigy.toml";

/// Dependency sections a declared install must satisfy. Optional dependencies
/// stay out: their absence is a valid platform outcome, so requiring them would
/// invent a refusal the lock never asked for.
const DECLARED_SECTIONS: [&str; 3] = ["dependencies", "devDependencies", "peerDependencies"];

/// Reported drift entries per refusal. The full count stays in the message.
const DRIFT_REPORT_LIMIT: usize = 5;

/// A reader that turns one manager's lock into resolved versions by name.
type LockReader = fn(&Path) -> Option<BTreeMap<String, String>>;

/// One declared dependency whose installed copy disagrees with the lock.
struct DriftedDependency {
    name: String,
    expected: String,
    observed: String,
}

struct DependencyDrift {
    package_root: PathBuf,
    lock_file: PathBuf,
    entries: Vec<DriftedDependency>,
    total: usize,
}

/// Refuse a managed host process start whose checkout install is stale.
///
/// `checkout_root` is the canonical checkout the policy was loaded for. The
/// refusal happens before any child starts, so a stale front end costs one
/// preflight read instead of a readiness budget.
pub(super) fn validate_dependency_install(
    checkout_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if policy.host_processes.is_empty() {
        return Ok(());
    }
    // The policy was loaded from this checkout already; a manifest that cannot
    // be re-read here has no declaration to check against, and the loader's own
    // error has already been reported on that path.
    let Ok(manifest) = effigy_manifest::load_task_manifest(&checkout_root.join(MANIFEST_FILE))
    else {
        return Ok(());
    };
    let Some(manager) = manifest
        .package_manager
        .as_ref()
        .and_then(|config| config.js)
    else {
        return Ok(());
    };
    if manager.install_command().is_none() {
        return Ok(());
    }
    let linked = linked_package_names(checkout_root);
    let mut package_roots = BTreeSet::new();
    for process in &policy.host_processes {
        let cwd = resolve_host_process_cwd(checkout_root, process)?;
        if let Some(root) = nearest_package_root(&cwd, checkout_root) {
            package_roots.insert(root);
        }
    }
    for root in package_roots {
        let Some(drift) = drifted_install(&root, checkout_root, manager, &linked) else {
            continue;
        };
        return Err(drift_refusal(&manifest, manager, &drift));
    }
    Ok(())
}

/// The nearest package root at or above the process working directory, staying
/// inside the checkout. A working directory with no `package.json` below the
/// checkout has no JavaScript install to compare.
fn nearest_package_root(cwd: &Path, checkout_root: &Path) -> Option<PathBuf> {
    let mut directory = Some(cwd);
    while let Some(current) = directory {
        if current.join("package.json").is_file() {
            return Some(current.to_path_buf());
        }
        if current == checkout_root {
            break;
        }
        directory = current.parent();
    }
    None
}

/// Packages this checkout deliberately links to a local checkout. A linked
/// package's version is the linked checkout's, not the lock's, so it is not
/// drift. An unreadable ledger claims nothing.
fn linked_package_names(checkout_root: &Path) -> BTreeSet<String> {
    effigy_deps::RepoLinkStateStore::for_checkout(checkout_root)
        .read()
        .map(|state| {
            state
                .links
                .iter()
                .flat_map(|link| link.packages.iter().map(|package| package.name.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn drifted_install(
    package_root: &Path,
    checkout_root: &Path,
    manager: ManifestJsPackageManager,
    linked: &BTreeSet<String>,
) -> Option<DependencyDrift> {
    let (lock_file, expected) = manager_lock(package_root, checkout_root, manager)?;
    let declared = declared_dependency_names(&package_root.join("package.json"));
    let mut entries = Vec::new();
    let mut total = 0;
    for name in declared {
        if linked.contains(&name) || !is_resolvable_package_name(&name) {
            continue;
        }
        // A declared dependency the checkout's lock does not resolve (a
        // workspace member, a file or git spec) carries no comparable version.
        let Some(expected_version) = expected.get(&name) else {
            continue;
        };
        let observed = installed_version(package_root, checkout_root, &name);
        // An absent copy is the doctor's missing-install finding, not drift: a
        // checkout may keep its JavaScript install beside the container that
        // runs it, and this preflight cannot tell that apart from never having
        // installed. Only a copy that exists and disagrees is proof.
        let Some(observed) = observed else {
            continue;
        };
        if observed == *expected_version {
            continue;
        }
        total += 1;
        if entries.len() < DRIFT_REPORT_LIMIT {
            entries.push(DriftedDependency {
                name,
                expected: expected_version.clone(),
                observed,
            });
        }
    }
    (total > 0).then(|| DependencyDrift {
        package_root: package_root.to_path_buf(),
        lock_file,
        entries,
        total,
    })
}

/// The manager's lock at or above `package_root`, with its resolved versions by
/// package name. Managers and lock formats this build cannot read return
/// `None`: an unread lock can never prove drift.
fn manager_lock(
    package_root: &Path,
    checkout_root: &Path,
    manager: ManifestJsPackageManager,
) -> Option<(PathBuf, BTreeMap<String, String>)> {
    let (file_name, parse): (&str, LockReader) = match manager {
        ManifestJsPackageManager::Bun => ("bun.lock", bun_lock_resolutions),
        ManifestJsPackageManager::Npm => ("package-lock.json", npm_lock_resolutions),
        // pnpm's and yarn's resolution formats have no reader here yet, and the
        // binary Bun lock is opaque; both stay unchanged rather than guess.
        ManifestJsPackageManager::Pnpm | ManifestJsPackageManager::Direct => return None,
    };
    let lock_file = nearest_ancestor_file(package_root, checkout_root, file_name)?;
    let resolved = parse(&lock_file)?;
    Some((lock_file, resolved))
}

fn nearest_ancestor_file(
    directory: &Path,
    checkout_root: &Path,
    file_name: &str,
) -> Option<PathBuf> {
    let mut current = Some(directory);
    while let Some(candidate) = current {
        let path = candidate.join(file_name);
        if path.is_file() {
            return Some(path);
        }
        if candidate == checkout_root {
            break;
        }
        current = candidate.parent();
    }
    None
}

/// Declared dependency names from one `package.json`.
fn declared_dependency_names(package_json: &Path) -> BTreeSet<String> {
    let Ok(raw) = fs::read_to_string(package_json) else {
        return BTreeSet::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return BTreeSet::new();
    };
    let mut names = BTreeSet::new();
    for section in DECLARED_SECTIONS {
        if let Some(entries) = value.get(section).and_then(serde_json::Value::as_object) {
            names.extend(entries.keys().cloned());
        }
    }
    names
}

/// A dependency name that can only name a directory under `node_modules`.
fn is_resolvable_package_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
}

/// The version of the first installed copy found on the resolution path:
/// this package root's `node_modules`, then each ancestor's inside the
/// checkout, which is how a hoisted workspace install resolves.
fn installed_version(package_root: &Path, checkout_root: &Path, name: &str) -> Option<String> {
    let mut current = Some(package_root);
    while let Some(directory) = current {
        let installed = directory.join("node_modules").join(name);
        if installed.is_dir() {
            let metadata = installed.join("package.json");
            if let Ok(raw) = fs::read_to_string(&metadata) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                    return value
                        .get("version")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned);
                }
            }
        }
        if directory == checkout_root {
            break;
        }
        current = directory.parent();
    }
    None
}

/// Bun's text lock, resolved versions by package name.
///
/// `packages` maps a name to `[<name>@<version>, ...]`. Entries that resolve to
/// something other than a plain version (`workspace:`, `link:`, `file:` or a
/// registry URL) are skipped: they have no registry version to compare.
fn bun_lock_resolutions(lock_file: &Path) -> Option<BTreeMap<String, String>> {
    let raw = fs::read_to_string(lock_file).ok()?;
    let value: serde_json::Value =
        serde_json::from_str(&jsonc_without_trailing_commas(&raw)).ok()?;
    let packages = value.get("packages")?.as_object()?;
    let mut resolved = BTreeMap::new();
    for (name, entry) in packages {
        let Some(spec) = entry
            .as_array()
            .and_then(|fields| fields.first())
            .and_then(|field| field.as_str())
        else {
            continue;
        };
        let Some(version) = spec.strip_prefix(&format!("{name}@")) else {
            continue;
        };
        if version.is_empty()
            || version.contains(':')
            || !version.starts_with(|ch: char| ch.is_ascii_digit())
        {
            continue;
        }
        resolved.insert(name.clone(), version.to_owned());
    }
    Some(resolved)
}

/// npm's `package-lock.json` (lockfile v2/v3), resolved versions by package
/// name for root-level and hoisted entries.
fn npm_lock_resolutions(lock_file: &Path) -> Option<BTreeMap<String, String>> {
    let raw = fs::read_to_string(lock_file).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let packages = value.get("packages")?.as_object()?;
    let mut resolved = BTreeMap::new();
    for (path, entry) in packages {
        let Some(name) = path.strip_prefix("node_modules/") else {
            continue;
        };
        if name.is_empty() || name.contains("/node_modules/") || !is_resolvable_package_name(name) {
            continue;
        }
        let Some(version) = entry.get("version").and_then(serde_json::Value::as_str) else {
            continue;
        };
        resolved.insert(name.to_owned(), version.to_owned());
    }
    Some(resolved)
}

/// JSON with the trailing commas Bun's text lock keeps.
fn jsonc_without_trailing_commas(raw: &str) -> String {
    let mut rendered = String::with_capacity(raw.len());
    let mut characters = raw.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(character) = characters.next() {
        if in_string {
            rendered.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if character == '"' {
            in_string = true;
            rendered.push(character);
            continue;
        }
        if character == ',' {
            let mut lookahead = characters.clone();
            while lookahead.peek().is_some_and(|next| next.is_whitespace()) {
                lookahead.next();
            }
            if matches!(lookahead.peek(), Some('}') | Some(']')) {
                continue;
            }
        }
        rendered.push(character);
    }
    rendered
}

/// The install command that makes this lock authoritative again.
fn frozen_install(manager: ManifestJsPackageManager) -> &'static str {
    match manager {
        ManifestJsPackageManager::Bun => "bun install --frozen-lockfile",
        ManifestJsPackageManager::Npm => "npm ci",
        ManifestJsPackageManager::Pnpm => "pnpm install --frozen-lockfile",
        ManifestJsPackageManager::Direct => "your JavaScript install",
    }
}

fn drift_refusal(
    manifest: &TaskManifest,
    manager: ManifestJsPackageManager,
    drift: &DependencyDrift,
) -> RunnerError {
    let lock_name = drift
        .lock_file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("the lockfile");
    let mut described = Vec::new();
    for entry in &drift.entries {
        described.push(format!(
            "`{}` is installed at {} but `{lock_name}` resolves {}",
            entry.name, entry.observed, entry.expected
        ));
    }
    if drift.total > drift.entries.len() {
        described.push(format!("and {} more", drift.total - drift.entries.len()));
    }
    let mut message = format!(
        "JavaScript dependencies for `{}` disagree with `{lock_name}`: {}; a stale install fails only after a host process starts, so this start was refused before any managed host process ran. Prepare dependencies with `{}`",
        drift.package_root.display(),
        described.join("; "),
        frozen_install(manager),
    );
    if manifest
        .bootstrap
        .as_ref()
        .is_some_and(|bootstrap| !bootstrap.setup.is_empty())
    {
        message.push_str(", or `effigy bootstrap` (this checkout's `[bootstrap].setup`)");
    }
    message.push_str(", then rerun this command.");
    RunnerError::task_invocation(message)
}

#[cfg(test)]
mod tests {
    use super::super::configuration::validate_host_process_preflight;
    use super::*;

    fn fixture(
        label: &str,
        package_manager: Option<&str>,
        host_process_cwd: Option<&str>,
    ) -> (effigy_test_support::Fixture, PathBuf) {
        let fixture = effigy_test_support::Fixture::new(label).expect("fixture");
        let root = fixture.path().join("project");
        fs::create_dir_all(root.join(".git")).expect("checkout");
        fs::write(
            root.join("compose.yml"),
            "services:\n  app:\n    image: example:test\n",
        )
        .expect("compose");
        let package_manager = package_manager
            .map(|manager| format!("[package_manager]\njs = \"{manager}\"\n"))
            .unwrap_or_default();
        let cwd = host_process_cwd
            .map(|cwd| format!("cwd = \"{cwd}\"\n"))
            .unwrap_or_default();
        fs::write(
            root.join("effigy.toml"),
            format!(
                r#"
{package_manager}
[bootstrap]
setup = ["bootstrap:deps"]

[containers]
default = "stack"

[containers.stack]
compose_file = "compose.yml"
primary_service = "app"

[[containers.stack.host_processes]]
name = "front"
run = "bun run dev"
{cwd}"#
            ),
        )
        .expect("manifest");
        (fixture, root)
    }

    fn policy(root: &Path) -> EffectiveContainerPolicy {
        effigy_containers::load_container_policy(root, Some("stack")).expect("policy")
    }

    fn write_package_json(directory: &Path, name: &str, dependencies: &[(&str, &str)]) {
        fs::create_dir_all(directory).expect("package root");
        let dependencies = dependencies
            .iter()
            .map(|(name, spec)| format!("\"{name}\": \"{spec}\""))
            .collect::<Vec<_>>()
            .join(",");
        fs::write(
            directory.join("package.json"),
            format!("{{\"name\":\"{name}\",\"dependencies\":{{{dependencies}}}}}"),
        )
        .expect("package.json");
    }

    fn write_installed(root: &Path, name: &str, version: &str) {
        let installed = root.join("node_modules").join(name);
        fs::create_dir_all(&installed).expect("installed package");
        fs::write(
            installed.join("package.json"),
            format!("{{\"name\":\"{name}\",\"version\":\"{version}\"}}"),
        )
        .expect("installed metadata");
    }

    /// Bun's text lock keeps trailing commas: the reader has to accept them.
    fn write_bun_lock(root: &Path, name: &str, version: &str) {
        fs::write(
            root.join("bun.lock"),
            format!(
                r#"{{
  "lockfileVersion": 1,
  "configVersion": 1,
  "workspaces": {{
    "": {{ "name": "app", "dependencies": {{ "{name}": "{version}" }}, }},
  }},
  "packages": {{
    "{name}": ["{name}@{version}", "", {{}}, "sha512-aaaa"],
  }},
}}"#
            ),
        )
        .expect("bun lock");
    }

    fn write_npm_lock(root: &Path, name: &str, version: &str) {
        fs::write(
            root.join("package-lock.json"),
            format!(
                r#"{{"name":"app","lockfileVersion":3,"packages":{{"":{{"name":"app"}},"node_modules/{name}":{{"version":"{version}"}}}}}}"#
            ),
        )
        .expect("npm lock");
    }

    /// The failure this preflight exists for: an installed package that the
    /// checkout's own lock resolves elsewhere.
    #[test]
    fn stale_installed_dependency_refuses_before_a_managed_host_process_starts() {
        let (_fixture, root) = fixture("drift-stale", Some("bun"), None);
        write_package_json(&root, "app", &[("praxis", "0.7.1")]);
        write_bun_lock(&root, "praxis", "0.7.1");
        write_installed(&root, "praxis", "0.6.0");
        let error = validate_host_process_preflight(&root, &policy(&root))
            .expect_err("a stale install refuses the start")
            .to_string();
        assert!(error.contains("praxis"), "{error}");
        assert!(
            error.contains("0.6.0") && error.contains("0.7.1"),
            "{error}"
        );
        assert!(
            error.contains("before any managed host process ran"),
            "{error}"
        );
        assert!(error.contains("bun install --frozen-lockfile"), "{error}");
        assert!(error.contains("effigy bootstrap"), "{error}");
    }

    #[test]
    fn an_install_matching_the_lock_passes() {
        let (_fixture, root) = fixture("drift-clean", Some("bun"), None);
        write_package_json(&root, "app", &[("praxis", "0.7.1")]);
        write_bun_lock(&root, "praxis", "0.7.1");
        write_installed(&root, "praxis", "0.7.1");
        validate_host_process_preflight(&root, &policy(&root)).expect("a matching install");
    }

    /// An install that never ran is the doctor's missing-install finding, not
    /// drift: this preflight only refuses a copy that exists and disagrees.
    #[test]
    fn an_absent_install_is_left_to_the_doctor_finding() {
        let (_fixture, root) = fixture("drift-missing", Some("bun"), None);
        write_package_json(&root, "app", &[("praxis", "0.7.1")]);
        write_bun_lock(&root, "praxis", "0.7.1");
        validate_host_process_preflight(&root, &policy(&root))
            .expect("no installed copy to compare");
    }

    /// The check follows the manifest declaration, never a universal
    /// `node_modules` rule: no `[package_manager].js` means no opinion.
    #[test]
    fn a_checkout_without_a_declared_package_manager_is_not_checked() {
        let (_fixture, root) = fixture("drift-undeclared", None, None);
        write_package_json(&root, "app", &[("praxis", "0.7.1")]);
        write_bun_lock(&root, "praxis", "0.7.1");
        write_installed(&root, "praxis", "0.6.0");
        validate_host_process_preflight(&root, &policy(&root)).expect("no declaration");
    }

    /// A lock this build cannot read proves nothing, so it refuses nothing.
    #[test]
    fn an_unreadable_lock_format_is_not_drift() {
        let (_fixture, root) = fixture("drift-pnpm", Some("pnpm"), None);
        write_package_json(&root, "app", &[("praxis", "0.7.1")]);
        fs::write(
            root.join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\nimporters:\n  .:\n    dependencies:\n      praxis:\n        specifier: 0.7.1\n",
        )
        .expect("pnpm lock");
        write_installed(&root, "praxis", "0.6.0");
        validate_host_process_preflight(&root, &policy(&root)).expect("unread lock");
    }

    /// npm's own lock resolution is compared the same way.
    #[test]
    fn npm_lock_resolution_drift_refuses_the_start() {
        let (_fixture, root) = fixture("drift-npm", Some("npm"), None);
        write_package_json(&root, "app", &[("praxis", "0.7.1")]);
        write_npm_lock(&root, "praxis", "0.7.1");
        write_installed(&root, "praxis", "0.6.0");
        let error = validate_host_process_preflight(&root, &policy(&root))
            .expect_err("npm drift refuses")
            .to_string();
        assert!(error.contains("package-lock.json"), "{error}");
        assert!(error.contains("npm ci"), "{error}");
    }

    /// A machine-local link is a deliberate different version, not drift.
    #[test]
    fn a_machine_linked_package_is_not_drift() {
        let (_fixture, root) = fixture("drift-linked", Some("bun"), None);
        write_package_json(&root, "app", &[("praxis", "0.7.1")]);
        write_bun_lock(&root, "praxis", "0.7.1");
        write_installed(&root, "praxis", "0.6.0");
        let library = root.join("local-praxis");
        fs::create_dir_all(&library).expect("library");
        let mut state = effigy_deps::RepoLinkState::empty();
        state.links.push(effigy_deps::DesiredDependencyLink {
            key: effigy_deps::DependencyLinkKey {
                manager: effigy_deps::PackageManager::Bun,
                consumer_repo: root.clone(),
                library_path: library.clone(),
            },
            mechanism: effigy_deps::LinkMechanism::BunLink,
            consumer_roots: vec![effigy_deps::ConsumerRoot {
                canonical_path: root.clone(),
            }],
            packages: vec![effigy_deps::DependencyPackage {
                name: "praxis".to_owned(),
                local_path: library,
                committed_sources: Vec::new(),
            }],
            cargo_resolutions: Vec::new(),
            cargo_ownership: None,
        });
        effigy_deps::RepoLinkStateStore::for_checkout(&root)
            .write(&state)
            .expect("link ledger");
        validate_host_process_preflight(&root, &policy(&root)).expect("a deliberate link");
    }

    /// The package root follows the process working directory, so a nested
    /// front end's install is the one compared.
    #[test]
    fn drift_is_reported_for_the_process_own_package_root() {
        let (_fixture, root) = fixture("drift-nested", Some("bun"), Some("web"));
        write_package_json(&root, "root-app", &[]);
        write_package_json(&root.join("web"), "web-app", &[("praxis", "0.7.1")]);
        write_bun_lock(&root, "praxis", "0.7.1");
        write_installed(&root.join("web"), "praxis", "0.6.0");
        let error = validate_host_process_preflight(&root, &policy(&root))
            .expect_err("the nested install is stale")
            .to_string();
        assert!(error.contains("web"), "{error}");
        assert!(error.contains("0.6.0"), "{error}");
    }
}
