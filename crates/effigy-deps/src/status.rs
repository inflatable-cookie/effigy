use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::bun::inventory_bun_file_dependencies;
use crate::cargo_unlink::{classify_lockfile, git_head_file};
use crate::{
    inspect_bun_peer_resolutions, inventory_cargo_consumer_roots, BunPeerDiagnostic,
    BunPeerResolutionStatus, BunRegistrationIndex, CargoLibraryInventory, CargoLockfileState,
    CargoPackageInventory, CargoPlanObserver, DependencyHealthSeverity, DependencyLinkReport,
    DependencyPackage, DependencyStatusReport, DependencyVerification, DesiredDependencyLink,
    DriftReason, GitCargoPlanObserver, LinkMechanism, ObservedDependencyLink, ObservedState,
    PackageManager, ReadOnlyProcess, RepoLinkState, VerificationEvidence, VerificationStatus,
};

const CARGO_MARKER_PREFIX: &str = "# >>> effigy deps cargo ";
const CARGO_MARKER_SUFFIX: &str = " >>>";

pub fn cargo_managed_block_markers(library_path: &Path) -> (String, String) {
    let identity = library_path.display();
    (
        format!("# >>> effigy deps cargo {identity} >>>"),
        format!("# <<< effigy deps cargo {identity} <<<"),
    )
}

pub fn inspect_dependency_status(
    repo_root: &Path,
    bun_home: &Path,
    state: &RepoLinkState,
    bun_index: &BunRegistrationIndex,
    process: &impl ReadOnlyProcess,
) -> Result<DependencyStatusReport, crate::DepsError> {
    let mut state = state.clone();
    state.normalize();
    let mut bun_index = bun_index.clone();
    bun_index.normalize();
    let cargo_guard_packages = state
        .links
        .iter()
        .filter(|link| link.mechanism == LinkMechanism::CargoPatch)
        .flat_map(|link| link.packages.iter().map(|package| package.name.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut links = Vec::new();
    for desired in &state.links {
        links.push(match desired.mechanism {
            LinkMechanism::CargoPatch => {
                inspect_cargo_link(repo_root, desired, &cargo_guard_packages, process)?
            }
            LinkMechanism::BunLink => inspect_bun_link(desired, bun_home, &bun_index)?,
        });
    }
    append_orphan_cargo_blocks(repo_root, &state, &mut links)?;
    append_orphan_bun_registrations(repo_root, &state, &bun_index, &mut links);
    append_bun_file_dependency_exposures(repo_root, &mut links)?;
    links.sort_by(|left, right| {
        left.desired
            .as_ref()
            .map(|desired| &desired.key)
            .cmp(&right.desired.as_ref().map(|desired| &desired.key))
            .then_with(|| {
                left.observed
                    .drift
                    .first()
                    .map(|reason| (&reason.code, &reason.package))
                    .cmp(
                        &right
                            .observed
                            .drift
                            .first()
                            .map(|reason| (&reason.code, &reason.package)),
                    )
            })
    });
    Ok(DependencyStatusReport { links })
}

fn inspect_cargo_link(
    repo_root: &Path,
    desired: &DesiredDependencyLink,
    cargo_guard_packages: &[String],
    process: &impl ReadOnlyProcess,
) -> Result<DependencyLinkReport, crate::DepsError> {
    if !desired.key.library_path.exists() {
        return Ok(problem_report(
            PackageManager::Cargo,
            Some(desired.clone()),
            ObservedState::Conflict,
            "library-path-missing",
            format!(
                "linked library path `{}` no longer exists",
                desired.key.library_path.display()
            ),
            None,
        ));
    }
    let library = CargoLibraryInventory {
        root: desired.key.library_path.clone(),
        packages: desired
            .packages
            .iter()
            .map(|package| CargoPackageInventory {
                id: package.name.clone(),
                name: package.name.clone(),
                manifest_path: package.local_path.join("Cargo.toml"),
                source: package.committed_sources.first().cloned(),
            })
            .collect(),
    };
    let config_path = desired.key.consumer_repo.join(".cargo/config.toml");
    let raw_config = read_optional_string(&config_path)?;
    let (start, end) = cargo_managed_block_markers(&desired.key.library_path);
    let marker_state = raw_config
        .as_deref()
        .map(|raw| (raw.contains(&start), raw.contains(&end)))
        .unwrap_or((false, false));
    let mut drift = cargo_config_hygiene(
        repo_root,
        desired,
        &config_path,
        raw_config.as_deref(),
        marker_state,
        process,
    )?;

    let mut observed_packages = Vec::new();
    let mut evidence = Vec::new();
    let consumer_roots = desired
        .consumer_roots
        .iter()
        .map(|root| root.canonical_path.clone())
        .collect::<Vec<_>>();
    let workspaces = match inventory_cargo_consumer_roots(
        &desired.key.consumer_repo,
        &consumer_roots,
        &library,
        true,
        process,
    ) {
        Ok(workspaces) => workspaces,
        Err(error) => {
            drift.push(error_reason(
                "cargo-resolution-inspection-failed",
                format!("Cargo dependency resolution could not be inspected: {error}"),
                None,
                vec![desired.key.consumer_repo.display().to_string()],
                Some(
                    "repair the Cargo config or dependency graph, then re-run `effigy deps status`",
                ),
            ));
            return Ok(report(
                desired.clone(),
                ObservedState::Conflict,
                Vec::new(),
                drift,
                Vec::new(),
            ));
        }
    };
    for package in &desired.packages {
        let expected = canonical_or_original(&package.local_path);
        let observed = workspaces
            .iter()
            .flat_map(|workspace| &workspace.resolved_packages)
            .filter(|candidate| candidate.name == package.name)
            .find_map(|candidate| {
                let path = candidate.manifest_path.parent()?;
                (canonical_or_original(path) == expected).then(|| path.to_path_buf())
            });
        if observed.is_some() {
            observed_packages.push(package.clone());
        }
        evidence.push(VerificationEvidence {
            package: package.name.clone(),
            consumer_root: None,
            committed_sources: package.committed_sources.clone(),
            expected_source: expected.display().to_string(),
            observed_source: observed.map(|path| path.display().to_string()),
            methods: vec!["cargo-metadata".to_owned()],
            message: None,
        });
    }

    let local = observed_packages.len();
    let total = desired.packages.len();
    let mut state = if marker_state.0 != marker_state.1 {
        ObservedState::Conflict
    } else if local == total && marker_state == (true, true) {
        ObservedState::Healthy
    } else if local > 0 && local < total {
        drift.push(error_reason(
            "cargo-link-partial-closure",
            format!("{local} of {total} desired crates resolve from the local checkout"),
            None,
            evidence_paths(&evidence),
            Some("re-run `effigy deps link cargo <library-path>` to restore the full closure"),
        ));
        ObservedState::Conflict
    } else if local == total {
        drift.push(error_reason(
            "cargo-local-resolution-unmanaged",
            "all desired crates resolve locally but the Effigy-managed Cargo block is absent",
            None,
            vec![config_path.display().to_string()],
            Some("remove the unmanaged patch, then run `effigy deps link cargo <library-path>`"),
        ));
        ObservedState::Conflict
    } else {
        drift.push(error_reason(
            "cargo-link-full-loss",
            "none of the desired crates resolve from the local checkout",
            None,
            evidence_paths(&evidence),
            Some("re-run `effigy deps link cargo <library-path>`"),
        ));
        ObservedState::Drifted
    };
    drift.extend(cargo_lock_hygiene(
        repo_root,
        desired,
        cargo_guard_packages,
        process,
    )?);
    if drift
        .iter()
        .any(|finding| finding.severity == DependencyHealthSeverity::Error)
    {
        state = ObservedState::Conflict;
    }
    Ok(report(
        desired.clone(),
        state,
        observed_packages,
        drift,
        evidence,
    ))
}

fn inspect_bun_link(
    desired: &DesiredDependencyLink,
    bun_home: &Path,
    bun_index: &BunRegistrationIndex,
) -> Result<DependencyLinkReport, crate::DepsError> {
    if !desired.key.library_path.exists() {
        return Ok(problem_report(
            PackageManager::Bun,
            Some(desired.clone()),
            ObservedState::Conflict,
            "library-path-missing",
            format!(
                "linked library path `{}` no longer exists",
                desired.key.library_path.display()
            ),
            None,
        ));
    }
    let mut observed_packages = Vec::new();
    let mut evidence = Vec::new();
    let mut correct_consumer_links = 0;
    let total_consumer_links = desired.packages.len() * desired.consumer_roots.len();
    let mut conflicts = Vec::new();

    for package in &desired.packages {
        let expected = canonical_or_original(&package.local_path);
        let registration_path = bun_home
            .join(".bun/install/global/node_modules")
            .join(&package.name);
        let registration_target = symlink_target(&registration_path);
        let indexed = bun_index.registrations.iter().find(|registration| {
            registration.package_name == package.name
                && canonical_or_original(&registration.package_path) == expected
                && registration.consumers.iter().any(|consumer| {
                    consumer.consumer_repo == desired.key.consumer_repo
                        && consumer.library_path == desired.key.library_path
                })
        });
        if registration_target.as_ref() != Some(&expected) || indexed.is_none() {
            conflicts.push(error_reason(
                "bun-registration-conflict",
                format!(
                    "Bun registration/index for `{}` does not point to `{}`",
                    package.name,
                    expected.display()
                ),
                Some(package.name.clone()),
                vec![registration_path.display().to_string()],
                Some("repair the conflicting registration, then re-run `effigy deps link bun <library-path>`"),
            ));
        }

        let mut package_links = 0;
        let mut last_target = None;
        for root in &desired.consumer_roots {
            let link_path = root.canonical_path.join("node_modules").join(&package.name);
            let target = symlink_target(&link_path);
            if target.as_ref() == Some(&expected) {
                correct_consumer_links += 1;
                package_links += 1;
            }
            last_target = target;
        }
        if package_links == desired.consumer_roots.len() {
            observed_packages.push(package.clone());
        }
        evidence.push(VerificationEvidence {
            package: package.name.clone(),
            consumer_root: None,
            committed_sources: package.committed_sources.clone(),
            expected_source: expected.display().to_string(),
            observed_source: last_target.map(|path| path.display().to_string()),
            methods: vec!["bun-symlink".to_owned()],
            message: None,
        });
    }

    let mut state = if !conflicts.is_empty() {
        ObservedState::Conflict
    } else if correct_consumer_links == total_consumer_links {
        ObservedState::Healthy
    } else if correct_consumer_links == 0 {
        conflicts.push(warning_reason(
            "bun-link-full-loss",
            "none of the desired Bun consumer symlinks remain; re-run deps link after install",
            None,
            evidence_paths(&evidence),
            Some("re-run `effigy deps link bun <library-path>`"),
        ));
        ObservedState::Drifted
    } else {
        conflicts.push(error_reason(
            "bun-link-partial-closure",
            format!(
                "{correct_consumer_links} of {total_consumer_links} desired Bun consumer symlinks remain"
            ),
            None,
            evidence_paths(&evidence),
            Some("re-run `effigy deps link bun <library-path>` to restore the full closure"),
        ));
        ObservedState::Conflict
    };
    let peer_diagnostics =
        match inspect_bun_peer_resolutions(&desired.key.consumer_repo, &desired.packages) {
            Ok(diagnostics) => diagnostics,
            Err(error) => {
                conflicts.push(error_reason(
                    "bun-peer-inspection-failed",
                    format!("Bun peer resolution could not be inspected: {error}"),
                    None,
                    desired
                        .packages
                        .iter()
                        .map(|package| {
                            package
                                .local_path
                                .join("package.json")
                                .display()
                                .to_string()
                        })
                        .collect(),
                    Some("repair the local package manifests, then re-run `effigy deps status`"),
                ));
                Vec::new()
            }
        };
    conflicts.extend(peer_health(&peer_diagnostics));
    conflicts.extend(bun_immutable_hygiene(desired)?);
    if conflicts
        .iter()
        .any(|finding| finding.severity == DependencyHealthSeverity::Error)
    {
        state = ObservedState::Conflict;
    }
    Ok(report_with_peers(
        desired.clone(),
        state,
        observed_packages,
        conflicts,
        evidence,
        peer_diagnostics,
    ))
}

fn cargo_config_hygiene(
    repo_root: &Path,
    desired: &DesiredDependencyLink,
    config_path: &Path,
    raw_config: Option<&str>,
    marker_state: (bool, bool),
    process: &impl ReadOnlyProcess,
) -> Result<Vec<DriftReason>, crate::DepsError> {
    let mut findings = Vec::new();
    if marker_state.0 != marker_state.1 {
        findings.push(error_reason(
            "cargo-managed-block-malformed",
            format!(
                "managed block markers are incomplete in `{}`",
                config_path.display()
            ),
            None,
            vec![config_path.display().to_string()],
            Some("remove the malformed block, then re-run `effigy deps link cargo <library-path>`"),
        ));
    }
    if repo_has_git_metadata(repo_root) {
        let observer = GitCargoPlanObserver::new(process);
        if observer.is_tracked(repo_root, config_path)? {
            findings.push(error_reason(
                "cargo-config-tracked",
                format!(
                    "machine-local Cargo config `{}` is tracked by Git",
                    config_path.display()
                ),
                None,
                vec![config_path.display().to_string()],
                Some("untrack `.cargo/config.toml`, keep it ignored, and do not commit the local patch"),
            ));
        }
    }
    if marker_state == (true, true) && !desired.cargo_resolutions.is_empty() {
        let (start, end) = cargo_managed_block_markers(&desired.key.library_path);
        match raw_config.and_then(|raw| managed_block_body(raw, &start, &end)) {
            Some(body) => match toml::from_str::<toml::Value>(body) {
                Ok(config) => {
                    for resolution in &desired.cargo_resolutions {
                        let observed = config
                            .get("patch")
                            .and_then(|patch| patch.get(&resolution.committed_source.identity))
                            .and_then(|source| source.get(&resolution.package))
                            .and_then(|package| package.get("path"))
                            .and_then(toml::Value::as_str);
                        let expected = resolution.local_path.display().to_string();
                        if observed != Some(expected.as_str()) {
                            findings.push(error_reason(
                                "cargo-managed-block-drift",
                                format!(
                                    "managed patch for `{}` under `{}` does not point to `{}`",
                                    resolution.package,
                                    resolution.committed_source.identity,
                                    resolution.local_path.display()
                                ),
                                Some(resolution.package.clone()),
                                vec![config_path.display().to_string()],
                                Some("re-run `effigy deps link cargo <library-path>` to rebuild the managed block"),
                            ));
                        }
                    }
                }
                Err(error) => findings.push(error_reason(
                    "cargo-managed-block-invalid",
                    format!("managed Cargo patch block is invalid TOML: {error}"),
                    None,
                    vec![config_path.display().to_string()],
                    Some("remove the invalid block, then re-run `effigy deps link cargo <library-path>`"),
                )),
            },
            None => findings.push(error_reason(
                "cargo-managed-block-malformed",
                "managed Cargo patch block boundaries could not be read",
                None,
                vec![config_path.display().to_string()],
                Some("remove the malformed block, then re-run `effigy deps link cargo <library-path>`"),
            )),
        }
    }
    Ok(findings)
}

fn cargo_lock_hygiene(
    repo_root: &Path,
    desired: &DesiredDependencyLink,
    cargo_guard_packages: &[String],
    process: &impl ReadOnlyProcess,
) -> Result<Vec<DriftReason>, crate::DepsError> {
    if !repo_has_git_metadata(repo_root) {
        return Ok(Vec::new());
    }
    let observer = GitCargoPlanObserver::new(process);
    let paths = std::iter::once(repo_root.join("Cargo.lock"))
        .chain(
            desired
                .consumer_roots
                .iter()
                .map(|root| root.canonical_path.join("Cargo.lock")),
        )
        .collect::<BTreeSet<_>>();
    let mut findings = Vec::new();
    for path in paths {
        if !path.exists() || !observer.is_tracked(repo_root, &path)? {
            continue;
        }
        let baseline = git_head_file(repo_root, &path, process)?;
        let current = fs::read_to_string(&path)
            .map_err(|error| crate::DepsError::io("read Cargo lockfile", &path, error))?;
        match classify_lockfile(&path, &baseline, &current, cargo_guard_packages)? {
            CargoLockfileState::Clean => {}
            CargoLockfileState::ActiveLinks => findings.push(error_reason(
                "cargo-lock-active-link-state",
                format!(
                    "tracked lockfile `{}` contains active local-link resolution; do not commit it",
                    path.display()
                ),
                None,
                vec![path.display().to_string()],
                Some("run `effigy deps unlink cargo <library-path>` and verify committed-source resolution before committing"),
            )),
            CargoLockfileState::UnexpectedDrift => findings.push(error_reason(
                "cargo-lock-unexpected-drift",
                format!(
                    "tracked lockfile `{}` contains changes outside the active linked package closure",
                    path.display()
                ),
                None,
                vec![path.display().to_string()],
                Some("separate unrelated lockfile edits before linking or unlinking; do not commit mixed local-link state"),
            )),
        }
    }
    Ok(findings)
}

fn managed_block_body<'a>(raw: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let start_index = raw.find(start)? + start.len();
    let end_index = raw[start_index..].find(end)? + start_index;
    (start_index <= end_index).then_some(raw[start_index..end_index].trim())
}

fn peer_health(diagnostics: &[BunPeerDiagnostic]) -> Vec<DriftReason> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.status == BunPeerResolutionStatus::Duplicate)
        .map(|diagnostic| {
            let evidence = diagnostic
                .consumer_resolution
                .iter()
                .chain(diagnostic.local_resolution.iter())
                .map(|path| path.display().to_string())
                .collect();
            error_reason(
                "bun-peer-duplicate-resolution",
                diagnostic.message.clone().unwrap_or_else(|| {
                    format!(
                        "peer `{}` resolves from multiple physical paths for `{}`",
                        diagnostic.peer, diagnostic.package
                    )
                }),
                Some(diagnostic.package.clone()),
                evidence,
                Some("remove the local peer copy and hoist/dedupe the peer in the consumer"),
            )
        })
        .collect()
}

fn bun_immutable_hygiene(
    desired: &DesiredDependencyLink,
) -> Result<Vec<DriftReason>, crate::DepsError> {
    let package_names = desired
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect::<Vec<_>>();
    let mut findings = Vec::new();
    for root in &desired.consumer_roots {
        let manifest_path = root.canonical_path.join("package.json");
        if let Some(raw) = read_optional_string(&manifest_path)? {
            let manifest: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(manifest) => manifest,
                Err(error) => {
                    findings.push(error_reason(
                        "bun-manifest-invalid",
                        format!(
                            "consumer manifest `{}` is invalid JSON: {error}",
                            manifest_path.display()
                        ),
                        None,
                        vec![manifest_path.display().to_string()],
                        Some("repair the consumer manifest, then re-run `effigy deps status`"),
                    ));
                    serde_json::Value::Null
                }
            };
            for section in [
                "dependencies",
                "devDependencies",
                "peerDependencies",
                "optionalDependencies",
            ] {
                for package in &package_names {
                    let linked = manifest
                        .get(section)
                        .and_then(|dependencies| dependencies.get(*package))
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|value| value.starts_with("link:"));
                    if linked {
                        findings.push(error_reason(
                            "bun-manifest-link-drift",
                            format!(
                                "consumer manifest `{}` contains committed `link:` state for `{package}`",
                                manifest_path.display()
                            ),
                            Some((*package).to_owned()),
                            vec![manifest_path.display().to_string()],
                            Some("restore the pinned dependency specifier and use save-less `effigy deps link bun <library-path>`"),
                        ));
                    }
                }
            }
        }
        for lock_path in [
            root.canonical_path.join("bun.lock"),
            root.canonical_path.join("bun.lockb"),
        ] {
            let Ok(raw) = fs::read(&lock_path) else {
                continue;
            };
            if std::str::from_utf8(&raw).is_err() {
                continue;
            }
            if raw.split(|byte| *byte == b'\n').any(|line| {
                contains_bytes(line, b"link:")
                    && package_names
                        .iter()
                        .any(|package| contains_bytes(line, package.as_bytes()))
            }) {
                findings.push(error_reason(
                    "bun-lock-link-drift",
                    format!(
                        "Bun lockfile `{}` contains committed `link:` state for the desired closure",
                        lock_path.display()
                    ),
                    None,
                    vec![lock_path.display().to_string()],
                    Some("restore the pinned lockfile and use save-less `effigy deps link bun <library-path>`"),
                ));
            }
        }
    }
    Ok(findings)
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn repo_has_git_metadata(repo_root: &Path) -> bool {
    repo_root.join(".git").exists()
}

fn evidence_paths(evidence: &[VerificationEvidence]) -> Vec<String> {
    evidence
        .iter()
        .filter_map(|item| item.observed_source.clone())
        .collect()
}

fn append_orphan_cargo_blocks(
    repo_root: &Path,
    state: &RepoLinkState,
    reports: &mut Vec<DependencyLinkReport>,
) -> Result<(), crate::DepsError> {
    let config_path = repo_root.join(".cargo/config.toml");
    let Some(config) = read_optional_string(&config_path)? else {
        return Ok(());
    };
    for library_path in config.lines().filter_map(|line| {
        line.trim()
            .strip_prefix(CARGO_MARKER_PREFIX)
            .and_then(|line| line.strip_suffix(CARGO_MARKER_SUFFIX))
    }) {
        let represented = state.links.iter().any(|link| {
            link.mechanism == LinkMechanism::CargoPatch
                && canonical_or_original(&link.key.consumer_repo)
                    == canonical_or_original(repo_root)
                && link.key.library_path == Path::new(library_path)
        });
        if !represented {
            reports.push(problem_report(
                PackageManager::Cargo,
                None,
                ObservedState::Conflict,
                "cargo-managed-block-without-ledger",
                format!(
                    "`{}` contains an Effigy-managed block for `{library_path}` but desired link state is absent",
                    config_path.display()
                ),
                None,
            ));
        }
    }
    Ok(())
}

fn append_orphan_bun_registrations(
    repo_root: &Path,
    state: &RepoLinkState,
    index: &BunRegistrationIndex,
    reports: &mut Vec<DependencyLinkReport>,
) {
    for registration in &index.registrations {
        let references_repo = registration.consumers.iter().any(|consumer| {
            canonical_or_original(&consumer.consumer_repo) == canonical_or_original(repo_root)
        });
        let represented = state.links.iter().any(|link| {
            link.mechanism == LinkMechanism::BunLink
                && canonical_or_original(&link.key.consumer_repo)
                    == canonical_or_original(repo_root)
                && link
                    .packages
                    .iter()
                    .any(|package| package.name == registration.package_name)
        });
        if references_repo && !represented {
            reports.push(problem_report(
                PackageManager::Bun,
                None,
                ObservedState::Conflict,
                "bun-registration-without-ledger",
                format!(
                    "Bun registration `{}` references this repo but desired link state is absent",
                    registration.package_name
                ),
                Some(registration.package_name.clone()),
            ));
        }
    }
}

fn append_bun_file_dependency_exposures(
    repo_root: &Path,
    reports: &mut Vec<DependencyLinkReport>,
) -> Result<(), crate::DepsError> {
    let consumer_repo = canonical_or_original(repo_root);
    for dependency in inventory_bun_file_dependencies(repo_root)? {
        append_bun_file_dependency_finder_metadata(&dependency, reports);
        let dependency_repo = containing_repo_root(&dependency.target_path);
        if dependency_repo == consumer_repo {
            continue;
        }
        let mut visible_packages = BTreeMap::new();
        for node_modules in dependency_node_modules_paths(&dependency.target_path, &dependency_repo)
        {
            for (package, link_path, external_target) in
                node_module_entries(&node_modules, &dependency_repo)?
            {
                visible_packages
                    .entry(package)
                    .or_insert((link_path, external_target));
            }
        }
        for (package, (link_path, external_target)) in visible_packages {
            if let Some(target_path) = external_target {
                reports.push(unowned_warning_report(
                    "bun-file-dependency-exposes-link",
                    format!(
                        "file dependency `{}` ({}) exposes linked package `{package}` from `{}`",
                        dependency.name,
                        dependency.specifier,
                        target_path.display()
                    ),
                    Some(package.clone()),
                    vec![
                        dependency.manifest_path.display().to_string(),
                        dependency.target_path.display().to_string(),
                        format!("{} -> {}", link_path.display(), target_path.display()),
                    ],
                    format!(
                        "unlink `{package}` in `{}` or add a consumer-level Bun override for it, then run `bun install`",
                        dependency_repo.display()
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Report macOS Finder metadata sitting inside a `file:` dependency tree.
///
/// Bun installs `file:` dependencies by copying the directory. Finder
/// droppings copy along with it and break the install inside a Linux
/// container, where they are neither expected nor removable after the fact.
/// Effigy does not own Bun's copy, so it names the offending paths up front.
fn append_bun_file_dependency_finder_metadata(
    dependency: &crate::bun::BunFileDependency,
    reports: &mut Vec<DependencyLinkReport>,
) {
    const REPORTED_PATH_LIMIT: usize = 10;
    let found = crate::bun::finder_metadata_paths(&dependency.target_path, REPORTED_PATH_LIMIT);
    if found.is_empty() {
        return;
    }
    let mut evidence = vec![dependency.target_path.display().to_string()];
    evidence.extend(found.iter().map(|path| path.display().to_string()));
    reports.push(unowned_warning_report(
        "bun-file-dependency-finder-metadata",
        format!(
            "file dependency `{}` ({}) carries macOS Finder metadata that Bun copies into the install",
            dependency.name, dependency.specifier
        ),
        None,
        evidence,
        format!(
            "remove Finder metadata from `{}` with `{}`, then re-run the install",
            dependency.target_path.display(),
            crate::bun::finder_metadata_cleanup_command(&dependency.target_path)
        ),
    ));
}

fn containing_repo_root(path: &Path) -> PathBuf {
    path.ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(canonical_or_original)
        .unwrap_or_else(|| canonical_or_original(path))
}

fn dependency_node_modules_paths(package_path: &Path, repo_root: &Path) -> Vec<PathBuf> {
    package_path
        .ancestors()
        .take_while(|ancestor| ancestor.starts_with(repo_root))
        .map(|ancestor| ancestor.join("node_modules"))
        .collect()
}

fn node_module_entries(
    node_modules: &Path,
    repo_root: &Path,
) -> Result<Vec<(String, PathBuf, Option<PathBuf>)>, crate::DepsError> {
    let Some(entries) = read_optional_directory(node_modules)? else {
        return Ok(Vec::new());
    };
    let mut links = Vec::new();
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if name.starts_with('@') {
            let Some(packages) = read_optional_directory(&entry.path())? else {
                continue;
            };
            for package in packages {
                let package_name = format!("{name}/{}", package.file_name().to_string_lossy());
                append_node_module_entry(&mut links, package_name, package.path(), repo_root);
            }
        } else {
            append_node_module_entry(&mut links, name, entry.path(), repo_root);
        }
    }
    links.sort();
    links.dedup();
    Ok(links)
}

fn read_optional_directory(path: &Path) -> Result<Option<Vec<fs::DirEntry>>, crate::DepsError> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(crate::DepsError::io("read node_modules", path, error)),
    };
    let mut collected = Vec::new();
    for entry in entries {
        collected.push(
            entry.map_err(|error| crate::DepsError::io("read node_modules entry", path, error))?,
        );
    }
    collected.sort_by_key(fs::DirEntry::file_name);
    Ok(Some(collected))
}

fn append_node_module_entry(
    links: &mut Vec<(String, PathBuf, Option<PathBuf>)>,
    package: String,
    link_path: PathBuf,
    repo_root: &Path,
) {
    let external_target =
        symlink_target(&link_path).filter(|target| !target.starts_with(repo_root));
    links.push((package, link_path, external_target));
}

fn report(
    desired: DesiredDependencyLink,
    state: ObservedState,
    packages: Vec<DependencyPackage>,
    drift: Vec<DriftReason>,
    evidence: Vec<VerificationEvidence>,
) -> DependencyLinkReport {
    report_with_peers(desired, state, packages, drift, evidence, Vec::new())
}

fn report_with_peers(
    desired: DesiredDependencyLink,
    state: ObservedState,
    packages: Vec<DependencyPackage>,
    drift: Vec<DriftReason>,
    evidence: Vec<VerificationEvidence>,
    peer_diagnostics: Vec<BunPeerDiagnostic>,
) -> DependencyLinkReport {
    DependencyLinkReport {
        manager: desired.key.manager,
        desired: Some(desired),
        observed: ObservedDependencyLink {
            state,
            packages,
            drift,
        },
        plan: None,
        verification: DependencyVerification {
            status: if state == ObservedState::Healthy {
                VerificationStatus::Passed
            } else {
                VerificationStatus::Failed
            },
            evidence,
        },
        peer_diagnostics,
    }
}

fn problem_report(
    manager: PackageManager,
    desired: Option<DesiredDependencyLink>,
    state: ObservedState,
    code: &str,
    message: impl Into<String>,
    package: Option<String>,
) -> DependencyLinkReport {
    let evidence = desired
        .as_ref()
        .map(|desired| vec![desired.key.library_path.display().to_string()])
        .unwrap_or_default();
    DependencyLinkReport {
        manager,
        desired,
        observed: ObservedDependencyLink {
            state,
            packages: Vec::new(),
            drift: vec![error_reason(
                code,
                message,
                package,
                evidence,
                status_remediation(code),
            )],
        },
        plan: None,
        verification: DependencyVerification {
            status: VerificationStatus::Failed,
            evidence: Vec::new(),
        },
        peer_diagnostics: Vec::new(),
    }
}

fn unowned_warning_report(
    code: &str,
    message: impl Into<String>,
    package: Option<String>,
    evidence: Vec<String>,
    remediation: impl Into<String>,
) -> DependencyLinkReport {
    DependencyLinkReport {
        manager: PackageManager::Bun,
        desired: None,
        observed: ObservedDependencyLink {
            state: ObservedState::Drifted,
            packages: Vec::new(),
            drift: vec![DriftReason {
                code: code.to_owned(),
                severity: DependencyHealthSeverity::Warning,
                message: message.into(),
                evidence,
                remediation: Some(remediation.into()),
                package,
            }],
        },
        plan: None,
        verification: DependencyVerification {
            status: VerificationStatus::Failed,
            evidence: Vec::new(),
        },
        peer_diagnostics: Vec::new(),
    }
}

fn error_reason(
    code: &str,
    message: impl Into<String>,
    package: Option<String>,
    evidence: Vec<String>,
    remediation: Option<&str>,
) -> DriftReason {
    health_reason(
        code,
        DependencyHealthSeverity::Error,
        message,
        package,
        evidence,
        remediation,
    )
}

fn warning_reason(
    code: &str,
    message: impl Into<String>,
    package: Option<String>,
    evidence: Vec<String>,
    remediation: Option<&str>,
) -> DriftReason {
    health_reason(
        code,
        DependencyHealthSeverity::Warning,
        message,
        package,
        evidence,
        remediation,
    )
}

fn health_reason(
    code: &str,
    severity: DependencyHealthSeverity,
    message: impl Into<String>,
    package: Option<String>,
    evidence: Vec<String>,
    remediation: Option<&str>,
) -> DriftReason {
    DriftReason {
        code: code.to_owned(),
        severity,
        message: message.into(),
        evidence,
        remediation: remediation.map(str::to_owned),
        package,
    }
}

fn status_remediation(code: &str) -> Option<&'static str> {
    match code {
        "library-path-missing" => Some(
            "restore the local library checkout or run the matching `effigy deps unlink <manager> <library-path>` command",
        ),
        "cargo-managed-block-without-ledger" => Some(
            "remove the orphaned Effigy-managed block after verifying no desired link should own it",
        ),
        "bun-registration-without-ledger" => Some(
            "verify the registration is unused, then unregister it from the local package directory",
        ),
        _ => None,
    }
}

fn read_optional_string(path: &Path) -> Result<Option<String>, crate::DepsError> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(crate::DepsError::io("read", path, error)),
    }
}

fn symlink_target(path: &Path) -> Option<PathBuf> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_symlink() {
        return None;
    }
    fs::canonicalize(path).ok()
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::{
        BunConsumerReference, BunRegistration, CargoExpectedResolution, CargoLinkOwnership,
        CommittedSource, CommittedSourceKind, ConsumerRoot, DependencyLinkKey, LinkMechanism,
        PackageManager, ProcessOutput, ProcessRequest,
    };

    struct NoProcess;

    impl ReadOnlyProcess for NoProcess {
        fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, crate::DepsError> {
            panic!("unexpected process request: {request:?}")
        }
    }

    struct FixtureProcess {
        outputs: BTreeMap<PathBuf, String>,
        requests: RefCell<Vec<ProcessRequest>>,
    }

    impl ReadOnlyProcess for FixtureProcess {
        fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, crate::DepsError> {
            self.requests.borrow_mut().push(request.clone());
            let index = request
                .args
                .iter()
                .position(|argument| argument == "--manifest-path")
                .unwrap();
            let manifest = PathBuf::from(&request.args[index + 1]);
            let output = self
                .outputs
                .iter()
                .find(|(path, _)| canonical_or_original(path) == canonical_or_original(&manifest))
                .map(|(_, output)| output.clone())
                .unwrap_or_else(|| panic!("no fixture for {}", manifest.display()));
            Ok(ProcessOutput {
                status: Some(0),
                stdout: output,
                stderr: String::new(),
            })
        }
    }

    struct HealthProcess {
        metadata: String,
        tracked: BTreeSet<PathBuf>,
        head_files: BTreeMap<PathBuf, String>,
        requests: RefCell<Vec<ProcessRequest>>,
    }

    impl ReadOnlyProcess for HealthProcess {
        fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, crate::DepsError> {
            self.requests.borrow_mut().push(request.clone());
            let stdout = match (
                request.program.as_str(),
                request.args.first().map(String::as_str),
            ) {
                ("cargo", _) => self.metadata.clone(),
                ("git", Some("ls-files")) => {
                    let path = PathBuf::from(request.args.last().unwrap());
                    if self.tracked.contains(&path) {
                        format!("{}\n", path.display())
                    } else {
                        String::new()
                    }
                }
                ("git", Some("show")) => request
                    .args
                    .get(1)
                    .and_then(|argument| argument.strip_prefix("HEAD:"))
                    .and_then(|path| self.head_files.get(Path::new(path)))
                    .cloned()
                    .unwrap_or_default(),
                _ => panic!("unexpected process request: {request:?}"),
            };
            Ok(ProcessOutput {
                status: Some(0),
                stdout,
                stderr: String::new(),
            })
        }
    }

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn desired(
        manager: PackageManager,
        consumer: &Path,
        library: &Path,
        package_names: &[&str],
    ) -> DesiredDependencyLink {
        DesiredDependencyLink {
            key: DependencyLinkKey {
                manager,
                consumer_repo: consumer.to_path_buf(),
                library_path: library.to_path_buf(),
            },
            mechanism: match manager {
                PackageManager::Cargo => LinkMechanism::CargoPatch,
                PackageManager::Bun => LinkMechanism::BunLink,
            },
            consumer_roots: vec![ConsumerRoot {
                canonical_path: consumer.to_path_buf(),
            }],
            packages: package_names
                .iter()
                .map(|name| DependencyPackage {
                    name: (*name).to_owned(),
                    local_path: library.join(name.trim_start_matches('@').replace('/', "-")),
                    committed_sources: vec![CommittedSource {
                        kind: CommittedSourceKind::Registry,
                        identity: "1.0.0".to_owned(),
                    }],
                })
                .collect(),
            cargo_resolutions: Vec::new(),
            cargo_ownership: (manager == PackageManager::Cargo).then_some(CargoLinkOwnership {
                config_created_by_effigy: true,
                cargo_dir_created_by_effigy: true,
            }),
        }
    }

    #[test]
    fn empty_state_produces_empty_status_without_processes() {
        let temp = TempDir::new().unwrap();
        let report = inspect_dependency_status(
            temp.path(),
            temp.path(),
            &RepoLinkState::empty(),
            &BunRegistrationIndex::empty(),
            &NoProcess,
        )
        .unwrap();
        assert!(report.links.is_empty());
    }

    #[test]
    fn missing_library_is_a_conflict_without_processes() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("missing");
        let link = desired(PackageManager::Cargo, temp.path(), &missing, &["core"]);
        let report = inspect_dependency_status(
            temp.path(),
            temp.path(),
            &RepoLinkState {
                schema: crate::REPO_LINK_STATE_SCHEMA.to_owned(),
                schema_version: crate::REPO_LINK_STATE_SCHEMA_VERSION,
                links: vec![link],
            },
            &BunRegistrationIndex::empty(),
            &NoProcess,
        )
        .unwrap();
        assert_eq!(report.links[0].observed.state, ObservedState::Conflict);
        assert_eq!(
            report.links[0].observed.drift[0].code,
            "library-path-missing"
        );
    }

    #[test]
    fn cargo_status_is_healthy_only_with_full_local_closure_and_managed_block() {
        let consumer = TempDir::new().unwrap();
        let library = TempDir::new().unwrap();
        let manifest = consumer.path().join("Cargo.toml");
        write(
            &manifest,
            "[package]\nname='consumer'\nversion='0.1.0'\n[dependencies]\ncore='1'\n",
        );
        let package_path = library.path().join("core");
        let package_manifest = package_path.join("Cargo.toml");
        write(
            &package_manifest,
            "[package]\nname='core'\nversion='1.0.0'\n",
        );
        let link = desired(
            PackageManager::Cargo,
            consumer.path(),
            library.path(),
            &["core"],
        );
        let (start, end) = cargo_managed_block_markers(library.path());
        write(
            &consumer.path().join(".cargo/config.toml"),
            &format!(
                "{start}\n[patch.\"https://example.test/core\"]\ncore={{path='{}'}}\n{end}\n",
                package_path.display()
            ),
        );
        let output = json!({
            "packages": [
                {"id":"consumer","name":"consumer","manifest_path":manifest,"source":null},
                {"id":"core","name":"core","manifest_path":package_manifest,"source":null}
            ],
            "workspace_members": ["consumer"],
            "workspace_root": consumer.path(),
            "resolve": {"nodes":[{"id":"consumer","deps":[{"pkg":"core"}]}]}
        })
        .to_string();
        let process = FixtureProcess {
            outputs: BTreeMap::from([(manifest, output)]),
            requests: RefCell::new(Vec::new()),
        };
        let state = RepoLinkState {
            schema: crate::REPO_LINK_STATE_SCHEMA.to_owned(),
            schema_version: crate::REPO_LINK_STATE_SCHEMA_VERSION,
            links: vec![link],
        };

        let before = fs::read(consumer.path().join(".cargo/config.toml")).unwrap();
        let report = inspect_dependency_status(
            consumer.path(),
            consumer.path(),
            &state,
            &BunRegistrationIndex::empty(),
            &process,
        )
        .unwrap();
        let after = fs::read(consumer.path().join(".cargo/config.toml")).unwrap();
        assert_eq!(report.links[0].observed.state, ObservedState::Healthy);
        assert_eq!(
            report.links[0].verification.status,
            VerificationStatus::Passed
        );
        assert_eq!(
            before, after,
            "status inspection must not write Cargo config"
        );
    }

    #[test]
    fn cargo_status_reports_config_and_lock_hygiene_without_writes() {
        let consumer = TempDir::new().unwrap();
        fs::create_dir(consumer.path().join(".git")).unwrap();
        let library = TempDir::new().unwrap();
        let manifest = consumer.path().join("Cargo.toml");
        write(
            &manifest,
            "[package]\nname='consumer'\nversion='0.1.0'\n[dependencies]\ncore='1'\n",
        );
        let package_path = library.path().join("core");
        let package_manifest = package_path.join("Cargo.toml");
        write(
            &package_manifest,
            "[package]\nname='core'\nversion='1.0.0'\n",
        );
        let mut link = desired(
            PackageManager::Cargo,
            consumer.path(),
            library.path(),
            &["core"],
        );
        link.cargo_resolutions = vec![CargoExpectedResolution {
            consumer_root: consumer.path().to_path_buf(),
            package: "core".to_owned(),
            committed_source: CommittedSource {
                kind: CommittedSourceKind::Git,
                identity: "https://example.test/core".to_owned(),
            },
            local_path: package_path.clone(),
        }];
        let (start, end) = cargo_managed_block_markers(library.path());
        let config_path = consumer.path().join(".cargo/config.toml");
        write(
            &config_path,
            &format!(
                "{start}\n[patch.\"https://example.test/core\"]\ncore={{path='/wrong/path'}}\n{end}\n"
            ),
        );
        let baseline_lock = "version = 3\n\n[[package]]\nname = \"consumer\"\nversion = \"0.1.0\"\ndependencies = [\"core\"]\n\n[[package]]\nname = \"core\"\nversion = \"1.0.0\"\nsource = \"git+https://example.test/core#abc\"\nchecksum = \"abc\"\n";
        let current_lock = "version = 3\n\n[[package]]\nname = \"consumer\"\nversion = \"0.1.0\"\ndependencies = [\"core\"]\n\n[[package]]\nname = \"core\"\nversion = \"1.0.0\"\n";
        let lock_path = consumer.path().join("Cargo.lock");
        write(&lock_path, current_lock);
        let metadata = json!({
            "packages": [
                {"id":"consumer","name":"consumer","manifest_path":manifest,"source":null},
                {"id":"core","name":"core","manifest_path":package_manifest,"source":null}
            ],
            "workspace_members": ["consumer"],
            "workspace_root": consumer.path(),
            "resolve": {"nodes":[{"id":"consumer","deps":[{"pkg":"core"}]}]}
        })
        .to_string();
        let process = HealthProcess {
            metadata,
            tracked: BTreeSet::from([
                PathBuf::from(".cargo/config.toml"),
                PathBuf::from("Cargo.lock"),
            ]),
            head_files: BTreeMap::from([(PathBuf::from("Cargo.lock"), baseline_lock.to_owned())]),
            requests: RefCell::new(Vec::new()),
        };
        let state = RepoLinkState {
            schema: crate::REPO_LINK_STATE_SCHEMA.to_owned(),
            schema_version: crate::REPO_LINK_STATE_SCHEMA_VERSION,
            links: vec![link],
        };
        let config_before = fs::read(&config_path).unwrap();
        let lock_before = fs::read(&lock_path).unwrap();

        let report = inspect_dependency_status(
            consumer.path(),
            consumer.path(),
            &state,
            &BunRegistrationIndex::empty(),
            &process,
        )
        .unwrap();
        let findings = &report.links[0].observed.drift;

        assert_eq!(report.links[0].observed.state, ObservedState::Conflict);
        for code in [
            "cargo-config-tracked",
            "cargo-managed-block-drift",
            "cargo-lock-active-link-state",
        ] {
            let finding = findings
                .iter()
                .find(|finding| finding.code == code)
                .unwrap();
            assert_eq!(finding.severity, DependencyHealthSeverity::Error);
            assert!(!finding.evidence.is_empty());
            assert!(finding.remediation.is_some());
        }
        assert_eq!(fs::read(&config_path).unwrap(), config_before);
        assert_eq!(fs::read(&lock_path).unwrap(), lock_before);
        assert!(process
            .requests
            .borrow()
            .iter()
            .all(|request| request.program == "cargo" || request.program == "git"));
    }

    #[test]
    fn orphan_managed_cargo_block_is_reported() {
        let temp = TempDir::new().unwrap();
        write(
            &temp.path().join(".cargo/config.toml"),
            "# >>> effigy deps cargo /missing >>>\n# <<< effigy deps cargo /missing <<<\n",
        );
        let report = inspect_dependency_status(
            temp.path(),
            temp.path(),
            &RepoLinkState::empty(),
            &BunRegistrationIndex::empty(),
            &NoProcess,
        )
        .unwrap();
        assert_eq!(report.links.len(), 1);
        assert_eq!(
            report.links[0].observed.drift[0].code,
            "cargo-managed-block-without-ledger"
        );
    }

    #[cfg(unix)]
    mod unix {
        use std::os::unix::fs::symlink;

        use super::*;

        struct BunFixture {
            consumer: TempDir,
            library: TempDir,
            home: TempDir,
            state: RepoLinkState,
            index: BunRegistrationIndex,
        }

        impl BunFixture {
            fn new(package_names: &[&str]) -> Self {
                let consumer = TempDir::new().unwrap();
                let library = TempDir::new().unwrap();
                let home = TempDir::new().unwrap();
                let link = desired(
                    PackageManager::Bun,
                    consumer.path(),
                    library.path(),
                    package_names,
                );
                let mut registrations = Vec::new();
                for package in &link.packages {
                    fs::create_dir_all(&package.local_path).unwrap();
                    write(
                        &package.local_path.join("package.json"),
                        &format!("{{\"name\":\"{}\"}}\n", package.name),
                    );
                    let registration = home
                        .path()
                        .join(".bun/install/global/node_modules")
                        .join(&package.name);
                    fs::create_dir_all(registration.parent().unwrap()).unwrap();
                    symlink(&package.local_path, registration).unwrap();
                    registrations.push(BunRegistration {
                        package_name: package.name.clone(),
                        package_path: package.local_path.clone(),
                        effigy_created: true,
                        consumers: vec![BunConsumerReference {
                            consumer_repo: consumer.path().to_path_buf(),
                            library_path: library.path().to_path_buf(),
                        }],
                    });
                }
                Self {
                    consumer,
                    library,
                    home,
                    state: RepoLinkState {
                        schema: crate::REPO_LINK_STATE_SCHEMA.to_owned(),
                        schema_version: crate::REPO_LINK_STATE_SCHEMA_VERSION,
                        links: vec![link],
                    },
                    index: BunRegistrationIndex {
                        schema: crate::BUN_REGISTRATION_INDEX_SCHEMA.to_owned(),
                        schema_version: crate::BUN_REGISTRATION_INDEX_SCHEMA_VERSION,
                        registrations,
                    },
                }
            }

            fn link_consumer_package(&self, index: usize) {
                let link = &self.state.links[0];
                let package = &link.packages[index];
                let target = link.consumer_roots[0]
                    .canonical_path
                    .join("node_modules")
                    .join(&package.name);
                fs::create_dir_all(target.parent().unwrap()).unwrap();
                symlink(&package.local_path, target).unwrap();
            }

            fn inspect(&self) -> DependencyStatusReport {
                inspect_dependency_status(
                    &self.state.links[0].key.consumer_repo,
                    self.home.path(),
                    &self.state,
                    &self.index,
                    &NoProcess,
                )
                .unwrap()
            }
        }

        #[test]
        fn distinguishes_healthy_full_loss_and_partial_bun_closure() {
            let healthy = BunFixture::new(&["@scope/core", "@scope/protocol"]);
            healthy.link_consumer_package(0);
            healthy.link_consumer_package(1);
            assert_eq!(
                healthy.inspect().links[0].observed.state,
                ObservedState::Healthy
            );

            let lost = BunFixture::new(&["@scope/core", "@scope/protocol"]);
            let lost_report = lost.inspect();
            assert_eq!(lost_report.links[0].observed.state, ObservedState::Drifted);
            assert!(lost_report.links[0]
                .observed
                .drift
                .iter()
                .any(|reason| reason.code == "bun-link-full-loss"));

            let partial = BunFixture::new(&["@scope/core", "@scope/protocol"]);
            partial.link_consumer_package(0);
            let partial_report = partial.inspect();
            assert_eq!(
                partial_report.links[0].observed.state,
                ObservedState::Conflict
            );
            assert!(partial_report.links[0]
                .observed
                .drift
                .iter()
                .any(|reason| reason.code == "bun-link-partial-closure"));
        }

        #[test]
        fn bun_status_warns_when_file_dependency_exposes_external_package_links() {
            let temp = TempDir::new().unwrap();
            let consumer = temp.path().join("consumer");
            let hub = temp.path().join("hub");
            let hub_package = hub.join("packages/hub");
            let local_package = temp.path().join("poodle/packages/core");
            fs::create_dir_all(consumer.join(".git")).unwrap();
            fs::create_dir_all(hub.join(".git")).unwrap();
            fs::create_dir_all(&hub_package).unwrap();
            fs::create_dir_all(&local_package).unwrap();
            write(
                &consumer.join("package.json"),
                "{\"dependencies\":{\"@acme/hub\":\"file:../hub/packages/hub\"}}\n",
            );
            let linked_package = hub.join("node_modules/@acme/poodle");
            fs::create_dir_all(linked_package.parent().unwrap()).unwrap();
            symlink(&local_package, &linked_package).unwrap();
            let bun_store_package = hub.join("node_modules/.bun/svelte/node_modules/svelte");
            fs::create_dir_all(&bun_store_package).unwrap();
            symlink(&bun_store_package, hub.join("node_modules/svelte")).unwrap();
            let manifest_before = fs::read(consumer.join("package.json")).unwrap();

            let report = inspect_dependency_status(
                &consumer,
                temp.path(),
                &RepoLinkState::empty(),
                &BunRegistrationIndex::empty(),
                &NoProcess,
            )
            .unwrap();

            assert_eq!(report.links.len(), 1);
            let finding = &report.links[0].observed.drift[0];
            assert_eq!(finding.code, "bun-file-dependency-exposes-link");
            assert_eq!(finding.severity, DependencyHealthSeverity::Warning);
            assert_eq!(finding.package.as_deref(), Some("@acme/poodle"));
            assert!(finding.message.contains("file dependency `@acme/hub`"));
            assert!(finding.message.contains(local_package.to_str().unwrap()));
            assert!(finding
                .remediation
                .as_deref()
                .unwrap()
                .contains("consumer-level Bun override"));
            assert_eq!(
                fs::read(consumer.join("package.json")).unwrap(),
                manifest_before
            );
        }

        #[test]
        fn bun_status_reports_saved_link_churn_and_duplicate_peer_paths_without_writes() {
            let fixture = BunFixture::new(&["@scope/core"]);
            fixture.link_consumer_package(0);
            let package = &fixture.state.links[0].packages[0];
            write(
                &package.local_path.join("package.json"),
                "{\"name\":\"@scope/core\",\"peerDependencies\":{\"svelte\":\"^5\"}}\n",
            );
            write(
                &fixture.consumer.path().join("package.json"),
                "{\"dependencies\":{\"@scope/core\":\"link:../core\",\"svelte\":\"^5\"}}\n",
            );
            write(
                &fixture.consumer.path().join("bun.lock"),
                "\"@scope/core\" = \"link:../core\"\n",
            );
            write(
                &fixture
                    .consumer
                    .path()
                    .join("node_modules/svelte/package.json"),
                "{\"name\":\"svelte\",\"version\":\"5.56.8\"}\n",
            );
            write(
                &package.local_path.join("node_modules/svelte/package.json"),
                "{\"name\":\"svelte\",\"version\":\"5.53.10\"}\n",
            );
            let manifest_before = fs::read(fixture.consumer.path().join("package.json")).unwrap();
            let lock_before = fs::read(fixture.consumer.path().join("bun.lock")).unwrap();

            let report = fixture.inspect();
            let link = &report.links[0];

            assert_eq!(link.observed.state, ObservedState::Conflict);
            for code in [
                "bun-manifest-link-drift",
                "bun-lock-link-drift",
                "bun-peer-duplicate-resolution",
            ] {
                let finding = link
                    .observed
                    .drift
                    .iter()
                    .find(|finding| finding.code == code)
                    .unwrap();
                assert_eq!(finding.severity, DependencyHealthSeverity::Error);
                assert!(!finding.evidence.is_empty());
                assert!(finding.remediation.is_some());
            }
            assert_eq!(link.peer_diagnostics.len(), 1);
            assert_eq!(
                link.peer_diagnostics[0].status,
                BunPeerResolutionStatus::Duplicate
            );
            assert_ne!(
                link.peer_diagnostics[0].consumer_resolution,
                link.peer_diagnostics[0].local_resolution
            );
            assert_eq!(
                fs::read(fixture.consumer.path().join("package.json")).unwrap(),
                manifest_before
            );
            assert_eq!(
                fs::read(fixture.consumer.path().join("bun.lock")).unwrap(),
                lock_before
            );
            assert!(fixture.library.path().exists());
        }
    }
}
