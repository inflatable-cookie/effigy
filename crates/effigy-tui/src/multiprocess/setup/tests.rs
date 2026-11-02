use super::{parse_vt_emulator_flag, select_process_tabs};

#[test]
fn select_process_tabs_falls_back_to_default_order() {
    let selected = select_process_tabs(
        vec!["api".to_owned(), "db".to_owned()],
        Vec::<String>::new(),
    );
    assert_eq!(selected, vec!["api".to_owned(), "db".to_owned()]);
}

#[test]
fn select_process_tabs_prefers_explicit_tab_order() {
    let selected = select_process_tabs(
        vec!["api".to_owned(), "db".to_owned()],
        vec!["db".to_owned(), "api".to_owned(), "shell".to_owned()],
    );
    assert_eq!(
        selected,
        vec!["db".to_owned(), "api".to_owned(), "shell".to_owned()]
    );
}

/// The VT emulator flag accepts the documented enabled spellings, and the
/// known disabled spellings — in any case — turn it off.
#[test]
fn parse_vt_emulator_flag_maps_enabled_and_disabled_values() {
    for (value, expected) in [
        (None, true),
        (Some("true"), true),
        (Some("1"), true),
        (Some("yes"), true),
        (Some("0"), false),
        (Some("false"), false),
        (Some("FALSE"), false),
    ] {
        assert_eq!(parse_vt_emulator_flag(value), expected, "flag {value:?}");
    }
}
