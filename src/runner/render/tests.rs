use super::resolve_text_color_enabled;
use crate::ui::OutputMode;

#[test]
fn resolve_text_color_enabled_follows_output_mode_when_no_color_unset() {
    assert!(!resolve_text_color_enabled(OutputMode::Auto, false, false));
    assert!(resolve_text_color_enabled(OutputMode::Auto, true, false));
    assert!(resolve_text_color_enabled(OutputMode::Always, false, false));
    assert!(!resolve_text_color_enabled(OutputMode::Never, true, false));
}

#[test]
fn resolve_text_color_enabled_no_color_disables_styles() {
    let modes = [OutputMode::Auto, OutputMode::Always, OutputMode::Never];
    for mode in modes {
        assert!(!resolve_text_color_enabled(mode, false, true));
        assert!(!resolve_text_color_enabled(mode, true, true));
    }
}
