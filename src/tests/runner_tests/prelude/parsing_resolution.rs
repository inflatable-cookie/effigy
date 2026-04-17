use super::runtime::RunnerError;

pub(in crate::runner::tests) fn assert_catalog_prefix_not_found(
    err: RunnerError,
    expected_prefix: &str,
    expected_available: &[&str],
) {
    match err {
        RunnerError::TaskCatalogPrefixNotFound { prefix, available } => {
            assert_eq!(prefix, expected_prefix);
            assert_eq!(
                available,
                expected_available
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>()
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}
