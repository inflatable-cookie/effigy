use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_deps::{
    inspect_dependency_status, is_process_timeout, BoundedReadOnlyProcess,
    BunRegistrationIndexStore, DependencyHealthSeverity, DependencyLinkReport,
    DependencyStatusReport, DepsError, LinkMechanism, PackageManager, ReadOnlyProcess,
    RepoLinkStateStore,
};

use crate::{check_id, remediation, DoctorFinding, DoctorSeverity, DoctorState};

/// Phase name recorded when the shared doctor deadline expires inside the
/// Cargo metadata or Git identity children owned by dependency inspection.
const DEPENDENCY_HEALTH_PHASE: &str = "dependency_health";

pub fn dependency_health_findings(
    repo_root: &Path,
    report: &DependencyStatusReport,
) -> Vec<DoctorFinding> {
    report
        .links
        .iter()
        .flat_map(|link| link_findings(repo_root, link))
        .collect()
}

/// Runs dependency health inside the shared doctor deadline.
///
/// Returns `false` when the deadline expired inside (or before) the owned
/// Cargo metadata and Git identity children, so the caller reports a
/// non-zero partial result instead of continuing into deep work.
///
/// The Cargo and Git children themselves are bounded by the remaining budget
/// and their process trees are terminated on expiry; the post-inspection
/// deadline re-check below is only a secondary guard.
pub(super) fn run_dependency_health_check(
    repo_root: &Path,
    state: &mut DoctorState,
    deadline: Option<Instant>,
) -> bool {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        state.record_budget_exhausted(DEPENDENCY_HEALTH_PHASE, Duration::ZERO);
        return false;
    }
    let started = Instant::now();
    let process = BoundedReadOnlyProcess::with_deadline(deadline);
    match inspect_with(repo_root, &process) {
        Ok(report) => {
            for finding in dependency_health_findings(repo_root, &report) {
                state.add_finding(finding);
            }
        }
        Err(InspectionFailure::BudgetExhausted) => {
            state.record_budget_exhausted(DEPENDENCY_HEALTH_PHASE, started.elapsed());
            return false;
        }
        Err(InspectionFailure::Failed(error)) => state.add_check_error(
            check_id::DEPENDENCY_LINK_HEALTH,
            format!(
                "manager=all; mechanism=all; library=<unknown>; consumer_roots={}; packages=<unknown>; observed=inspection-failed; detail={error}",
                repo_root.display()
            ),
            "Repair the machine-local dependency-link state, then run `effigy deps status` and `effigy doctor` again.",
        ),
    }
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        state.record_budget_exhausted(DEPENDENCY_HEALTH_PHASE, started.elapsed());
        return false;
    }
    true
}

enum InspectionFailure {
    BudgetExhausted,
    Failed(String),
}

fn inspect_with(
    repo_root: &Path,
    process: &impl ReadOnlyProcess,
) -> Result<DependencyStatusReport, InspectionFailure> {
    let state = RepoLinkStateStore::for_checkout(repo_root)
        .read()
        .map_err(map_inspection_error)?;
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
        InspectionFailure::Failed("HOME is not set; cannot inspect Bun links".to_owned())
    })?;
    let bun_index = BunRegistrationIndexStore::for_home(&home)
        .read()
        .map_err(map_inspection_error)?;
    inspect_dependency_status(repo_root, &home, &state, &bun_index, process)
        .map_err(map_inspection_error)
}

fn map_inspection_error(error: DepsError) -> InspectionFailure {
    if is_process_timeout(&error) {
        InspectionFailure::BudgetExhausted
    } else {
        InspectionFailure::Failed(error.to_string())
    }
}

fn link_findings(repo_root: &Path, link: &DependencyLinkReport) -> Vec<DoctorFinding> {
    if link.observed.drift.is_empty() {
        return vec![DoctorFinding {
            check_id: check_id::DEPENDENCY_LINK_HEALTH.to_owned(),
            severity: DoctorSeverity::Info,
            evidence: context(repo_root, link, None),
            remediation: remediation::NO_ACTION_REQUIRED.to_owned(),
            fixable: false,
        }];
    }

    link.observed
        .drift
        .iter()
        .map(|reason| {
            let evidence = context(repo_root, link, reason.package.as_deref());
            let evidence = format!(
                "{evidence}; reason={}; detail={}; evidence={}",
                reason.code,
                reason.message,
                joined_or_placeholder(&reason.evidence),
            );
            DoctorFinding {
                check_id: check_id::DEPENDENCY_LINK_HEALTH.to_owned(),
                severity: doctor_severity(reason.severity),
                evidence,
                remediation: reason.remediation.clone().unwrap_or_else(|| {
                    "Run `effigy deps status` for the exact link repair evidence.".to_owned()
                }),
                fixable: false,
            }
        })
        .collect()
}

fn context(repo_root: &Path, link: &DependencyLinkReport, package: Option<&str>) -> String {
    // A committed path or `file:` local is not an Effigy-managed link, so it
    // carries its own identity instead of the ledger's.
    let mechanism = link
        .desired
        .as_ref()
        .map(|desired| desired.mechanism.as_str().to_owned())
        .or_else(|| {
            link.committed_local
                .as_ref()
                .map(|local| local.mechanism.as_str().to_owned())
        })
        .unwrap_or_else(|| mechanism_for(link.manager).as_str().to_owned());
    let library = link
        .desired
        .as_ref()
        .map(|desired| desired.key.library_path.display().to_string())
        .or_else(|| {
            link.committed_local
                .as_ref()
                .map(|local| local.library_path.display().to_string())
        })
        .unwrap_or_else(|| "<untracked>".to_owned());
    let consumers = link
        .desired
        .as_ref()
        .map(|desired| {
            desired
                .consumer_roots
                .iter()
                .map(|root| root.canonical_path.display().to_string())
                .collect::<Vec<_>>()
        })
        .or_else(|| {
            link.committed_local.as_ref().map(|local| {
                local
                    .consumer_roots
                    .iter()
                    .map(|root| root.canonical_path.display().to_string())
                    .collect::<Vec<_>>()
            })
        })
        .filter(|consumers| !consumers.is_empty())
        .unwrap_or_else(|| vec![repo_root.display().to_string()]);
    let packages = link
        .desired
        .as_ref()
        .map(|desired| {
            desired
                .packages
                .iter()
                .map(|package| package.name.clone())
                .collect::<Vec<_>>()
        })
        .or_else(|| {
            link.committed_local.as_ref().map(|_| {
                link.observed
                    .packages
                    .iter()
                    .map(|package| package.name.clone())
                    .collect::<Vec<_>>()
            })
        })
        .filter(|packages| !packages.is_empty())
        .unwrap_or_else(|| {
            package
                .map(|package| vec![package.to_owned()])
                .unwrap_or_default()
        });
    format!(
        "manager={}; mechanism={}; library={library}; consumer_roots={}; packages={}; package={}; observed={}",
        link.manager.as_str(),
        mechanism,
        joined_or_placeholder(&consumers),
        joined_or_placeholder(&packages),
        package.unwrap_or("<none>"),
        link.observed.state.as_str(),
    )
}

fn mechanism_for(manager: PackageManager) -> LinkMechanism {
    match manager {
        PackageManager::Cargo => LinkMechanism::CargoPatch,
        PackageManager::Bun => LinkMechanism::BunLink,
    }
}

fn doctor_severity(severity: DependencyHealthSeverity) -> DoctorSeverity {
    match severity {
        DependencyHealthSeverity::Information => DoctorSeverity::Info,
        DependencyHealthSeverity::Warning => DoctorSeverity::Warning,
        DependencyHealthSeverity::Error => DoctorSeverity::Error,
    }
}

fn joined_or_placeholder(values: &[String]) -> String {
    if values.is_empty() {
        "<none>".to_owned()
    } else {
        values.join(" | ")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use effigy_deps::{
        ConsumerRoot, DependencyLinkKey, DependencyPackage, DependencyVerification,
        DesiredDependencyLink, DriftReason, ObservedDependencyLink, ObservedState,
        VerificationStatus,
    };

    use super::*;

    #[test]
    fn healthy_link_maps_to_visible_information_with_full_context() {
        let report = fixture_report(ObservedState::Healthy, Vec::new());

        let findings = dependency_health_findings(Path::new("/consumer"), &report);

        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.severity, DoctorSeverity::Info);
        assert!(finding.evidence.contains("manager=bun"));
        assert!(finding.evidence.contains("mechanism=bun-link"));
        assert!(finding.evidence.contains("library=/library"));
        assert!(finding.evidence.contains("consumer_roots=/consumer"));
        assert!(finding.evidence.contains("packages=@scope/core"));
        assert!(finding.evidence.contains("observed=healthy"));
        assert_eq!(finding.remediation, remediation::NO_ACTION_REQUIRED);

        let mut state = DoctorState::new();
        state.add_finding(finding.clone());
        let summary = state.summarize();
        let doctor_report =
            state.into_report("/consumer".to_owned(), summary, Vec::new(), Vec::new());
        let text = crate::render::render_text(&doctor_report, false).expect("render doctor text");
        let rendered = crate::render::render_json(&doctor_report).expect("render doctor json");
        let json: serde_json::Value = serde_json::from_str(&rendered).expect("doctor json");
        assert!(text.contains(check_id::DEPENDENCY_LINK_HEALTH));
        assert_eq!(json["sections"][0]["severity"], "info");
        assert_eq!(json["findings"][0]["evidence"], finding.evidence);
    }

    #[test]
    fn drift_maps_shared_severity_evidence_package_and_remediation_exactly() {
        let report = fixture_report(
            ObservedState::Conflict,
            vec![DriftReason {
                code: "bun-peer-duplicate-resolution".to_owned(),
                severity: DependencyHealthSeverity::Error,
                message: "Svelte resolves from two paths".to_owned(),
                evidence: vec![
                    "/consumer/node_modules/svelte".to_owned(),
                    "/library/node_modules/svelte".to_owned(),
                ],
                remediation: Some("hoist/dedupe Svelte".to_owned()),
                package: Some("@scope/core".to_owned()),
            }],
        );

        let findings = dependency_health_findings(Path::new("/consumer"), &report);

        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.severity, DoctorSeverity::Error);
        assert!(finding
            .evidence
            .contains("reason=bun-peer-duplicate-resolution"));
        assert!(finding.evidence.contains("package=@scope/core"));
        assert!(finding.evidence.contains("/consumer/node_modules/svelte"));
        assert!(finding.evidence.contains("/library/node_modules/svelte"));
        assert_eq!(finding.remediation, "hoist/dedupe Svelte");
    }

    #[test]
    fn expired_deadline_records_budget_exhausted_without_inspection() {
        use crate::report::DoctorCheckRunState;

        let mut state = DoctorState::new();
        let complete = run_dependency_health_check(
            Path::new("/nonexistent-repo-root"),
            &mut state,
            Some(
                std::time::Instant::now()
                    .checked_sub(std::time::Duration::from_secs(1))
                    .expect("past deadline"),
            ),
        );

        assert!(!complete);
        assert!(!state.is_complete());
        let run = state.check_runs.last().expect("budget run recorded");
        assert_eq!(run.name, "dependency_health");
        assert_eq!(run.state, DoctorCheckRunState::BudgetExhausted);
    }

    #[test]
    fn shared_warning_stays_a_doctor_warning() {
        let report = fixture_report(
            ObservedState::Missing,
            vec![DriftReason {
                code: "bun-link-full-loss".to_owned(),
                severity: DependencyHealthSeverity::Warning,
                message: "all package links are missing".to_owned(),
                evidence: Vec::new(),
                remediation: Some("run link again".to_owned()),
                package: None,
            }],
        );

        let findings = dependency_health_findings(Path::new("/consumer"), &report);

        assert_eq!(findings[0].severity, DoctorSeverity::Warning);
        assert_eq!(findings[0].remediation, "run link again");
    }

    #[test]
    fn doctor_text_and_json_render_the_same_dependency_health_contract() {
        let dependency_report = fixture_report(
            ObservedState::Conflict,
            vec![DriftReason {
                code: "bun-saved-link-manifest-churn".to_owned(),
                severity: DependencyHealthSeverity::Error,
                message: "package.json contains a saved link specifier".to_owned(),
                evidence: vec!["/consumer/package.json".to_owned()],
                remediation: Some("restore package.json and re-link without --save".to_owned()),
                package: Some("@scope/core".to_owned()),
            }],
        );
        let findings = dependency_health_findings(Path::new("/consumer"), &dependency_report);
        let expected_evidence = findings[0].evidence.clone();
        let expected_remediation = findings[0].remediation.clone();
        let mut state = DoctorState::new();
        for finding in findings {
            state.add_finding(finding);
        }
        let summary = state.summarize();
        let doctor_report =
            state.into_report("/consumer".to_owned(), summary, Vec::new(), Vec::new());

        let text = crate::render::render_text(&doctor_report, true).expect("render doctor text");
        let rendered = crate::render::render_json(&doctor_report).expect("render doctor json");
        let json: serde_json::Value = serde_json::from_str(&rendered).expect("doctor json");

        assert!(text.contains(&expected_evidence));
        assert!(text.contains(&expected_remediation));
        assert_eq!(
            json["findings"][0]["check_id"],
            check_id::DEPENDENCY_LINK_HEALTH
        );
        assert_eq!(json["findings"][0]["severity"], "error");
        assert_eq!(json["findings"][0]["evidence"], expected_evidence);
        assert_eq!(json["findings"][0]["remediation"], expected_remediation);
    }

    fn fixture_report(state: ObservedState, drift: Vec<DriftReason>) -> DependencyStatusReport {
        let desired = DesiredDependencyLink {
            key: DependencyLinkKey {
                manager: PackageManager::Bun,
                consumer_repo: PathBuf::from("/consumer"),
                library_path: PathBuf::from("/library"),
            },
            mechanism: LinkMechanism::BunLink,
            consumer_roots: vec![ConsumerRoot {
                canonical_path: PathBuf::from("/consumer"),
            }],
            packages: vec![DependencyPackage {
                name: "@scope/core".to_owned(),
                local_path: PathBuf::from("/library/packages/core"),
                committed_sources: Vec::new(),
            }],
            cargo_resolutions: Vec::new(),
            cargo_ownership: None,
        };
        DependencyStatusReport {
            links: vec![DependencyLinkReport {
                committed_local: None,
                manager: PackageManager::Bun,
                desired: Some(desired),
                observed: ObservedDependencyLink {
                    state,
                    packages: Vec::new(),
                    drift,
                },
                plan: None,
                verification: DependencyVerification {
                    status: if state == ObservedState::Healthy {
                        VerificationStatus::Passed
                    } else {
                        VerificationStatus::Failed
                    },
                    evidence: Vec::new(),
                },
                peer_diagnostics: Vec::new(),
            }],
        }
    }
}
