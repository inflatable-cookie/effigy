use effigy_changelog as changelog;

/// The live CHANGELOG formats and validates as a document. The library's own
/// unit tests carry section parsing, extraction and analysis.
#[test]
fn format_then_validate_roundtrip() {
    let content = include_str!("../CHANGELOG.md");
    let parsed = changelog::parse(content).expect("should parse");
    let formatted = changelog::format(&parsed);
    let reparsed = changelog::parse(&formatted).expect("should parse formatted");
    let diagnostics = changelog::validate(&reparsed, &formatted);

    assert!(
        diagnostics.is_empty(),
        "formatted changelog should be valid, got: {:?}",
        diagnostics
    );
}
