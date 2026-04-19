use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
struct ReleasedSurfaceTransition {
    source_baseline_tag: String,
    target_release_tag: String,
    target_release_kind: String,
    requires_migration_notes: bool,
    intentional_breaks: Vec<IntentionalBreak>,
}

#[derive(Deserialize)]
struct IntentionalBreak {
    surface: String,
    change: String,
    migration_note: String,
}

fn load_transition() -> ReleasedSurfaceTransition {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released_surface/v0.3.0/transition.json");
    let fixture = fs::read_to_string(&fixture_path).expect("read released surface transition");
    serde_json::from_str(&fixture).expect("parse released surface transition")
}

#[test]
fn v0_3_transition_contract_targets_v0_2_13_as_the_compatibility_floor() {
    let transition = load_transition();

    assert_eq!(transition.source_baseline_tag, "v0.2.13");
    assert_eq!(transition.target_release_tag, "v0.3.0");
    assert_eq!(transition.target_release_kind, "minor");
    assert!(transition.requires_migration_notes);
}

#[test]
fn v0_3_transition_contract_records_wrapper_retirement_break() {
    let transition = load_transition();

    assert!(transition
        .intentional_breaks
        .iter()
        .any(|intentional_break| {
            intentional_break.surface == "legacy release wrapper scripts"
                && intentional_break
                    .change
                    .contains("Removed compatibility-only shell entrypoints")
                && intentional_break
                    .migration_note
                    .contains("effigy release gates")
        }));
}

#[test]
fn v0_3_transition_contract_requires_complete_break_entries() {
    let transition = load_transition();

    for intentional_break in &transition.intentional_breaks {
        assert!(
            !intentional_break.surface.trim().is_empty(),
            "intentional break surface must not be empty"
        );
        assert!(
            !intentional_break.change.trim().is_empty(),
            "intentional break change must not be empty"
        );
        assert!(
            !intentional_break.migration_note.trim().is_empty(),
            "intentional break migration note must not be empty"
        );
    }
}
