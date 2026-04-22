use effigy_catalog::StarterResolver;
use effigy_manifest::TASK_MANIFEST_FILE;

use crate::BuiltinError;

/// Default starter emitted by `effigy init` when no name is supplied.
///
/// Kept behind a named constant so later batches can thread a caller-
/// selected starter name through the same loader without re-deriving the
/// default in multiple places.
const DEFAULT_STARTER: &str = "minimal";

/// Render the baseline `effigy.toml` scaffold by loading the default
/// starter from the bundled catalog.
///
/// For batch 1 of roadmap `g02.021` the only registered starter is
/// `minimal`, which declares exactly one file emitted at
/// `effigy.toml`. The invariant is validated here so that a later
/// descriptor change that silently breaks the single-file assumption
/// surfaces loudly instead of corrupting the existing CLI shape.
pub(super) fn render_init_scaffold() -> Result<String, BuiltinError> {
    let resolver = StarterResolver::new();
    let starter = resolver.resolve(DEFAULT_STARTER).map_err(|error| {
        BuiltinError::task_invocation(format!(
            "failed to load bundled `{DEFAULT_STARTER}` starter: {error}"
        ))
    })?;

    match starter.files.as_slice() {
        [file] if file.target == TASK_MANIFEST_FILE => Ok(file.contents.clone()),
        files => Err(BuiltinError::task_invocation(format!(
            "bundled `{DEFAULT_STARTER}` starter must emit exactly one \
             {TASK_MANIFEST_FILE} file in this batch; got {} file(s)",
            files.len()
        ))),
    }
}
