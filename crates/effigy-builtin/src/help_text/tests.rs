use super::{render_titled_help, HelpSection};

#[test]
fn render_titled_help_contract_is_stable() {
    let doc = render_titled_help(
        "bundle",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &["effigy bundle [--json]"],
            },
            HelpSection::Bulleted {
                heading: "Notes",
                items: &["first", "second"],
            },
        ],
    );

    assert_eq!(
        doc,
        "bundle Help\n\nUsage\neffigy bundle [--json]\n\nNotes\n- first\n- second"
    );
}
