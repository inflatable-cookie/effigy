use effigy_catalog::{Starter, StarterResolver};

use crate::BuiltinError;

/// Load a starter from the bundled catalog by name.
///
/// Batch 3 of roadmap `g02.021` generalizes init emission to multi-file
/// starters. Starters such as `minimal` (manifest + README) and `underlay`
/// share the same load path; the emitter in `init.rs`
/// handles per-file existence checks and write semantics.
pub(super) fn load_starter(starter_name: &str) -> Result<Starter, BuiltinError> {
    let resolver = StarterResolver::new();
    resolver.resolve(starter_name).map_err(|error| {
        BuiltinError::task_invocation(format!("failed to load starter `{starter_name}`: {error}"))
    })
}
