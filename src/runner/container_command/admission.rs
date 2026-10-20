//! The immutable admission snapshot of one bring-up. Route publication and
//! builder recovery compare against it, never against a baseline read after
//! runtime observation. The environment record, not a marker file, keeps the
//! admitted generation across invocations.

use std::path::Path;

use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;

#[derive(Debug)]
pub(in crate::runner) struct AdmissionContext {
    value: serde_json::Value,
    /// The snapshot names a published desired generation; execution-only
    /// recovery instead compares fresh render references.
    published: bool,
}

impl AdmissionContext {
    pub(in crate::runner) fn from_materialization(
        policy: &EffectiveContainerPolicy,
        admission: &effigy_containers::policy::admission::PolicyAdmission,
    ) -> Result<Self, RunnerError> {
        Ok(Self {
            value: admission.context_for_policy(policy)?,
            published: true,
        })
    }

    #[cfg(test)]
    pub(in crate::runner) fn capture(
        policy: &EffectiveContainerPolicy,
    ) -> Result<Self, RunnerError> {
        Ok(Self {
            value: context(policy)?,
            published: true,
        })
    }

    /// Execution-only recovery compares current source render references
    /// while retaining the stack's original admitted generation.
    pub(in crate::runner) fn workspace_builder(
        policy: &EffectiveContainerPolicy,
    ) -> Result<Self, RunnerError> {
        Ok(Self {
            value: context(policy)?,
            published: false,
        })
    }

    pub(in crate::runner) fn verify(
        &self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
    ) -> Result<(), RunnerError> {
        let verified = (|| -> Result<bool, RunnerError> {
            if self.published {
                effigy_containers::policy::render::verify(policy)?;
            } else {
                // Execution-only builder recovery may observe new source
                // inputs while retaining its original stack admission.
                let current =
                    effigy_containers::load_container_policy(repo_root, Some(&policy.name))?;
                if self.value != context(&current)? {
                    return Ok(false);
                }
            }
            Ok(self.value == context(policy)?)
        })();
        if !matches!(verified, Ok(true)) {
            return Err(RunnerError::task_invocation(
                "detached activation publication refused: admitted source/checkout/scope/Compose context changed or is unreadable; no routes published. Restore the admitted inputs and run `effigy container up --detach` again",
            ));
        }
        Ok(())
    }
}

fn context(policy: &EffectiveContainerPolicy) -> Result<serde_json::Value, RunnerError> {
    effigy_containers::policy::admission::generation_context(policy).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::AdmissionContext;

    const MANIFEST: &str = r#"
[systems]
default = "dev"

[systems.dev]
container = "web"
default_workspace = "app"
working_dir = "/workspace-root/app"
user = "dev"
home = "/home/dev"
mounts = []

[systems.dev.workspaces.app]

[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#;

    /// The exact publication seam of a managed detached activation: a strict
    /// admission verify against runtime Compose inputs while another process
    /// resolves the same policy and rewrites those inputs. The managed shell
    /// and exec readiness probes do exactly this once per second during a
    /// headless `dev:container` startup, and a truncating rewrite let the
    /// verify observe a torn input and refuse publication (#197). Atomic
    /// replacement keeps every read on one complete generation.
    #[test]
    fn concurrent_policy_resolutions_never_tear_admitted_compose_inputs() {
        let fixture = tempfile::tempdir().expect("temporary fixture root");
        let repo_root = fixture.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        std::fs::write(repo_root.join("effigy.toml"), MANIFEST).expect("write manifest");
        let containers_home = fixture.path().join("home/.effigy");
        // The writer thread resolves the policy through the same private home;
        // env (unlike a thread-local) reaches the spawned writer.
        let _env = crate::contract_test_support::EnvGuard::set_many(&[(
            "EFFIGY_TEST_CONTAINERS_HOME",
            Some(containers_home.display().to_string()),
        )]);
        effigy_containers::with_test_effigy_home(&containers_home, || {
            let policy = effigy_containers::prepare_container_policy(&repo_root, None)
                .expect("admission policy resolution");
            let context = AdmissionContext::capture(&policy).expect("capture admission");
            context
                .verify(&repo_root, &policy)
                .expect("fresh context verifies before the race");

            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let writer_stop = stop.clone();
            let writer_root = repo_root.clone();
            let writer = std::thread::spawn(move || {
                while !writer_stop.load(std::sync::atomic::Ordering::Relaxed) {
                    effigy_containers::load_container_policy(&writer_root, None)
                        .expect("writer policy resolution");
                }
            });
            let mut clean_reads = 0usize;
            let failure = loop {
                match context.verify(&repo_root, &policy) {
                    Ok(()) => clean_reads += 1,
                    Err(error) => break Some(error),
                }
                if clean_reads >= 2000 {
                    break None;
                }
            };
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
            writer.join().expect("writer joins");
            assert!(
                failure.is_none(),
                "publication verify raced a concurrent policy rewrite after {clean_reads} clean reads: {}",
                failure.expect("failure is set").to_string().replace('\n', " ")
            );
        });
    }
}
