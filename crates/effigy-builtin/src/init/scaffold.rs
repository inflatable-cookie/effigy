use effigy_catalog::StarterResolver;
use effigy_manifest::TASK_MANIFEST_FILE;

use crate::BuiltinError;

/// Render the manifest scaffold for the named starter by loading it from
/// the bundled starter catalog.
///
/// For batch 2 of roadmap `g02.021` the registered catalog still only
/// contains `minimal`, which declares exactly one file emitted at
/// `effigy.toml`. The single-file invariant is validated here so a later
/// starter that ships a multi-file shape surfaces clearly instead of
/// silently corrupting the CLI contract; batch 3 extends emission to
/// multi-file shapes and relaxes this check.
pub(super) fn render_starter_scaffold(starter_name: &str) -> Result<String, BuiltinError> {
    let resolver = StarterResolver::new();
    let starter = resolver.resolve(starter_name).map_err(|error| {
        BuiltinError::task_invocation(format!("failed to load starter `{starter_name}`: {error}"))
    })?;

    match starter.files.as_slice() {
        [file] if file.target == TASK_MANIFEST_FILE => Ok(file.contents.clone()),
        files => Err(BuiltinError::task_invocation(format!(
            "starter `{starter_name}` must emit exactly one {TASK_MANIFEST_FILE} file in this batch; got {} file(s)",
            files.len()
        ))),
    }
}
