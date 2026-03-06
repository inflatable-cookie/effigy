pub(super) fn write_doctor_non_zero(
    f: &mut std::fmt::Formatter<'_>,
    error_count: usize,
) -> std::fmt::Result {
    write!(f, "doctor found {error_count} error finding(s)")
}

pub(super) fn write_builtin_test_non_zero(
    f: &mut std::fmt::Formatter<'_>,
    failures: &[(String, Option<i32>)],
) -> std::fmt::Result {
    let rendered = failures
        .iter()
        .map(|(target, code)| match code {
            Some(value) => format!("{target}: exit={value}"),
            None => format!("{target}: terminated"),
        })
        .collect::<Vec<String>>()
        .join(", ");
    write!(f, "one or more built-in test targets failed: {rendered}")
}

pub(super) fn write_builtin_scan_non_zero(
    f: &mut std::fmt::Formatter<'_>,
    finding_count: usize,
) -> std::fmt::Result {
    write!(f, "scan found {finding_count} matching file(s)")
}
