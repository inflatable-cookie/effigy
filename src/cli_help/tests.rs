use super::truncate_path_for_header;

#[test]
fn truncate_path_for_header_keeps_tail_when_space_is_tight() {
    assert_eq!(
        truncate_path_for_header("/Users/tom/Dev/projects/effigy", 18),
        "…/projects/effigy"
    );
}
