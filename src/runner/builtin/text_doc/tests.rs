use super::TextDoc;

#[test]
fn text_doc_contract_is_stable() {
    let mut doc = TextDoc::new();
    doc.line("header")
        .kv("mode", "apply")
        .bullet("entry")
        .blank()
        .line("done");
    assert_eq!(doc.finish(), "header\nmode: apply\n- entry\n\ndone");
}
