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

#[test]
fn sections_keep_one_blank_line_between_them_whatever_their_shape() {
    let doc = render_titled_help(
        "scan",
        &[
            HelpSection::Bulleted {
                heading: "Surfaces",
                items: &[],
            },
            HelpSection::Plain {
                heading: "Usage",
                lines: &[],
            },
            HelpSection::Bulleted {
                heading: "Notes",
                items: &["one"],
            },
        ],
    );

    // An empty section still renders its heading, and empty content never
    // collapses the separating blank line into the heading.
    assert_eq!(doc, "scan Help\n\nSurfaces\n\nUsage\n\nNotes\n- one");
}

#[test]
fn a_panel_without_sections_is_only_its_title() {
    assert_eq!(render_titled_help("fmt", &[]), "fmt Help");
}
