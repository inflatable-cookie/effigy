use super::*;
use effigy_catalog::pack::{with_test_effigy_home, PackSelection, PackSelectionReason};
use std::fs;

fn temp_repo(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-catalog-command-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    root
}

/// Resolver plus selection for a repo with no pack store at all.
fn baseline_layers(root: &Path) -> CatalogLayers {
    let home = root.join("home");
    fs::create_dir_all(&home).expect("mkdir home");
    with_test_effigy_home(&home, || catalog_layers(root))
}

#[test]
fn catalog_list_reports_bundled_fragments() {
    let root = temp_repo("list");
    let layers = baseline_layers(&root);
    let rendered = run_service_list(&layers.resolver, &layers.selection, false).expect("list");
    assert!(rendered.contains("[service]"));
    assert!(rendered.contains("php-fpm [bundled]"));
}

#[test]
fn catalog_extract_defaults_to_project_override_dir() {
    let root = temp_repo("extract");
    let layers = baseline_layers(&root);
    let rendered =
        run_service_extract(&root, &layers.resolver, "nginx", None, false).expect("extract");

    assert!(rendered.contains("infra/dev/catalog/nginx"));
    assert!(root.join("infra/dev/catalog/nginx/service.toml").exists());
    assert!(root
        .join("infra/dev/catalog/nginx/compose.fragment.yml")
        .exists());
}

#[test]
fn baseline_list_reports_no_store_selection_in_json() {
    let root = temp_repo("baseline-json");
    let layers = baseline_layers(&root);
    let payload: serde_json::Value = serde_json::from_str(
        &run_service_list(&layers.resolver, &layers.selection, true).expect("list"),
    )
    .expect("json");

    assert_eq!(payload["schema"], "effigy.service.list.v1");
    assert_eq!(payload["selection"]["layer"], "compiled-baseline");
    assert_eq!(payload["selection"]["reason"], "no-store");
    assert_eq!(payload["selection"]["fallback"], false);
    assert!(payload["fragments"]
        .as_array()
        .expect("fragments")
        .iter()
        .any(|fragment| fragment["name"] == "postgres" && fragment["source"] == "bundled"));
}

#[test]
fn unhealthy_active_pack_warns_in_text_and_reports_a_reason_in_json() {
    let root = temp_repo("fallback-render");
    let layers = baseline_layers(&root);
    // Render against a fallback selection directly: the store-level paths that
    // produce one are proven in `effigy-catalog`; this asserts both surfaces
    // actually say so rather than silently showing baseline fragments.
    let selection = PackSelection {
        reason: PackSelectionReason::FallbackMissingContent,
        active: None,
        detail: Some("installed pack `p-1-0-0-abc` content is missing".to_owned()),
        store_root: Some(root.join("home/catalog-packs/v1")),
    };

    let text = run_service_list(&layers.resolver, &selection, false).expect("text");
    assert!(
        text.contains("[warn] active catalog pack is unhealthy"),
        "{text}"
    );
    assert!(text.contains("effigy service pack reset"), "{text}");

    let payload: serde_json::Value =
        serde_json::from_str(&run_service_list(&layers.resolver, &selection, true).expect("json"))
            .expect("json");
    assert_eq!(payload["selection"]["fallback"], true);
    assert_eq!(payload["selection"]["reason"], "fallback-missing-content");
    assert_eq!(payload["selection"]["layer"], "compiled-baseline");
    assert!(payload["selection"]["detail"]
        .as_str()
        .expect("detail")
        .contains("p-1-0-0-abc"));
}
