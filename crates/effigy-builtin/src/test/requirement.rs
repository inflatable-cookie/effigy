//! Pure storage requirements from the same suite resolver used by execution.
use super::*;
use effigy_manifest::{ManifestManagedRun, ManifestManagedRunStep};
use std::path::PathBuf;

#[derive(Debug)]
pub struct TestRunSource {
    pub root: PathBuf,
    pub label: String,
    pub run: ManifestManagedRun,
}

#[derive(Debug)]
pub enum TestRunRequirement {
    Observe,
    Checkout,
    /// Routing is local; the engine must classify these declared descendants.
    Local(Vec<TestRunSource>),
}

pub fn test_run_requirement(
    selector: &TaskSelector,
    args: &TaskRuntimeArgs,
    root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<TestRunRequirement, BuiltinError> {
    let (flags, passthrough) = planning::extract_builtin_test_flags(&args.passthrough);
    if flags.plan_mode {
        return Ok(TestRunRequirement::Observe);
    }
    // Classification only: nothing runs here, so no scoped alias resolution or
    // refusal belongs to this pass (effigy#431).
    let targets = planning::resolve_builtin_test_targets(selector, root, catalogs, None)?;
    let runnable = planning::collect_builtin_test_runnable_targets(&targets.targets);
    let selected = match suite_selection::select_builtin_test_suite(runnable, passthrough) {
        Ok(selected) => selected.runnable,
        // Execution owns the existing selection-error response. No work starts.
        Err(_) => return Ok(TestRunRequirement::Observe),
    };
    reject_unusable_suite_runtimes(&selected)?;
    if selected.iter().any(|suite| {
        !suite.nested_invocation
            && matches!(
                suite.runtime,
                planning::BuiltinTargetRuntime::Container { .. }
            )
    }) {
        return Ok(TestRunRequirement::Checkout);
    }
    let mut sources = Vec::new();
    for runnable in selected {
        let configured = catalogs
            .iter()
            .find(|catalog| catalog.catalog_root == runnable.root)
            .and_then(|catalog| catalog.manifest.test.as_ref())
            .and_then(|test| {
                test.suites
                    .iter()
                    .find(|(name, _)| {
                        effigy_tasks::normalize_builtin_test_suite(name).unwrap_or(name)
                            == runnable.runner
                    })
                    .map(|(_, suite)| suite)
            });
        let mut steps = Vec::<ManifestManagedRunStep>::new();
        if let Some(suite) = configured {
            steps.extend_from_slice(suite.setup());
            match suite.managed_run() {
                Some(ManifestManagedRun::Sequence(run)) => steps.extend_from_slice(run),
                Some(ManifestManagedRun::Command(command)) => {
                    steps.push(ManifestManagedRunStep::Command(command.clone()))
                }
                None => steps.push(ManifestManagedRunStep::Command(runnable.command)),
            }
            steps.extend_from_slice(suite.teardown());
        } else {
            steps.push(ManifestManagedRunStep::Command(runnable.command));
        }
        sources.push(TestRunSource {
            root: runnable.root,
            label: runnable.runner,
            run: ManifestManagedRun::Sequence(steps),
        });
    }
    Ok(TestRunRequirement::Local(sources))
}
