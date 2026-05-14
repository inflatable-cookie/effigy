use effigy_catalog::{Starter, StarterResolver};

use crate::BuiltinError;

/// Load a starter from the bundled catalog by name.
///
/// Multi-file starters share the same load path; the emitter in `init.rs`
/// handles per-file existence checks and write semantics.
pub(super) fn load_starter(starter_name: &str) -> Result<Starter, BuiltinError> {
    let resolver = StarterResolver::new();
    resolver.resolve(starter_name).map_err(|error| {
        BuiltinError::task_invocation(format!("failed to load starter `{starter_name}`: {error}"))
    })
}
