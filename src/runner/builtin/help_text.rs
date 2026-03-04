pub(super) enum HelpSection<'a> {
    Plain {
        heading: &'a str,
        lines: &'a [&'a str],
    },
    Bulleted {
        heading: &'a str,
        items: &'a [&'a str],
    },
}

pub(super) fn render_titled_help(title: &str, sections: &[HelpSection<'_>]) -> String {
    let mut lines = vec![format!("{title} Help"), String::new()];
    for (index, section) in sections.iter().enumerate() {
        match section {
            HelpSection::Plain {
                heading,
                lines: body,
            } => {
                lines.push((*heading).to_owned());
                lines.extend(body.iter().map(|line| (*line).to_owned()));
            }
            HelpSection::Bulleted { heading, items } => {
                lines.push((*heading).to_owned());
                lines.extend(items.iter().map(|item| format!("- {item}")));
            }
        }
        if index + 1 != sections.len() {
            lines.push(String::new());
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
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
}
