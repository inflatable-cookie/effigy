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
