use super::TextDoc;

#[test]
fn text_doc_contract_is_stable() {
    let mut doc = TextDoc::new();
    doc.line("header").blank().line("done");
    assert_eq!(doc.finish(), "header\n\ndone");
}

#[test]
fn text_doc_owns_the_list_prefix_and_document_line_breaks() {
    let mut doc = TextDoc::new();
    doc.line("Commands").bullets(&["first", "second"]);
    assert_eq!(doc.finish(), "Commands\n- first\n- second");
}

#[test]
fn an_empty_document_renders_as_the_empty_string() {
    assert_eq!(TextDoc::new().finish(), "");
    // A trailing blank line is a blank line, not a trailing newline.
    let mut doc = TextDoc::new();
    doc.line("only").blank();
    assert_eq!(doc.finish(), "only\n");
}
