use super::{render_titled_help, HelpSection};

#[test]
fn render_titled_help_contract_is_stable() {
    let doc = render_titled_help(
        "demo",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &["effigy demo [--json]"],
            },
            HelpSection::Bulleted {
                heading: "Notes",
                items: &["first", "second"],
            },
        ],
    );

    assert_eq!(
        doc,
        "demo Help\n\nUsage\neffigy demo [--json]\n\nNotes\n- first\n- second"
    );
}
