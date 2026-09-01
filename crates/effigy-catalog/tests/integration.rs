//! Integration tests for the catalog crate.
//!
//! These tests use the bundled catalog fragments to verify the full
//! resolve → validate → render → assemble pipeline.
//!
//! The body of the suite is split into themed sibling modules under
//! [`integration/`]. This entry file owns shared helpers used by more
//! than one of those modules.

use effigy_catalog::fragment::CatalogResolver;

#[path = "integration/fragments.rs"]
mod fragments;
#[path = "integration/pack_layer.rs"]
mod pack_layer;
#[path = "integration/services.rs"]
mod services;
#[path = "integration/structure.rs"]
mod structure;
#[path = "integration/workspace.rs"]
mod workspace;

/// Helper: create a resolver that only uses bundled fragments.
pub(crate) fn bundled_resolver() -> CatalogResolver {
    CatalogResolver::new(None, None)
}

/// Parse the assembled compose YAML back into a `serde_yaml::Value` and
/// validate its structure matches what Docker Compose expects.
pub(crate) fn validate_compose_structure(yaml: &str) -> serde_yaml::Value {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml)
        .unwrap_or_else(|e| panic!("assembled compose is not valid YAML:\n{e}\n---\n{yaml}"));

    // Must have top-level 'services' key.
    assert!(
        doc.get("services").is_some(),
        "compose missing 'services' key:\n{yaml}"
    );

    // Services must be a mapping.
    assert!(
        doc.get("services").unwrap().is_mapping(),
        "services is not a mapping:\n{yaml}"
    );

    // If volumes key exists, it must be a mapping.
    if let Some(volumes) = doc.get("volumes") {
        assert!(volumes.is_mapping(), "volumes is not a mapping:\n{yaml}");
    }

    doc
}

/// Validate a single service definition has the expected structure.
pub(crate) fn validate_service(doc: &serde_yaml::Value, name: &str) -> serde_yaml::Value {
    let services = doc.get("services").unwrap();
    let svc = services.get(name).unwrap_or_else(|| {
        panic!(
            "service '{name}' not found in compose. Available: {:?}",
            services.as_mapping().unwrap().keys().collect::<Vec<_>>()
        )
    });
    assert!(svc.is_mapping(), "service '{name}' is not a mapping");
    svc.clone()
}
