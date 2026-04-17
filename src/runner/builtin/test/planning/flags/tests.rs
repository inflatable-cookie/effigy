use super::extract_builtin_test_flags;

#[test]
fn extract_builtin_test_flags_treats_double_dash_as_passthrough_boundary() {
    let (flags, passthrough) = extract_builtin_test_flags(&[
        "--plan".to_owned(),
        "--".to_owned(),
        "managed".to_owned(),
        "--package".to_owned(),
        "catalog_a-db".to_owned(),
    ]);

    assert!(flags.plan_mode);
    assert_eq!(passthrough, vec!["managed", "--package", "catalog_a-db"]);
}
