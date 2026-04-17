//! Thin re-export shim — the real types live in `effigy-manifest`.
//!
//! The previous home for `LoadedCatalog`, `TaskSelection`, and
//! `DeferredCommand` was here, as runner-local `pub(in crate::runner)`
//! types. They moved into `effigy-manifest` (alongside `TaskManifest`)
//! so the upcoming `effigy-managed` extraction can depend on them
//! without reaching into runner internals.
//!
//! The existing ~78 runner call sites continue to work through this
//! shim. New code should prefer `effigy_manifest::{LoadedCatalog, ...}`
//! directly.

pub(in crate::runner) use effigy_manifest::{DeferredCommand, LoadedCatalog, TaskSelection};
pub(in crate::runner) use effigy_tasks::{CatalogSelectionMode, TaskRuntimeArgs, TaskSelector};
