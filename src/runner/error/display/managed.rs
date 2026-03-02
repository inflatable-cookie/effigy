pub(super) fn write_task_managed_unsupported_mode(
    f: &mut std::fmt::Formatter<'_>,
    task: &str,
    mode: &str,
) -> std::fmt::Result {
    write!(
        f,
        "task `{task}` declares unsupported managed mode `{mode}` (expected `tui`)"
    )
}

pub(super) fn write_task_managed_profile_not_found(
    f: &mut std::fmt::Formatter<'_>,
    task: &str,
    profile: &str,
    available: &[String],
) -> std::fmt::Result {
    write!(
        f,
        "managed task `{task}` profile `{profile}` not found (available: {})",
        available.join(", ")
    )
}

pub(super) fn write_task_managed_profile_empty(
    f: &mut std::fmt::Formatter<'_>,
    task: &str,
    profile: &str,
) -> std::fmt::Result {
    write!(
        f,
        "managed task `{task}` profile `{profile}` has no processes configured"
    )
}

pub(super) fn write_task_managed_process_not_found(
    f: &mut std::fmt::Formatter<'_>,
    task: &str,
    profile: &str,
    process: &str,
) -> std::fmt::Result {
    write!(
        f,
        "managed task `{task}` profile `{profile}` references undefined process `{process}`"
    )
}

pub(super) fn write_task_managed_process_invalid_definition(
    f: &mut std::fmt::Formatter<'_>,
    task: &str,
    process: &str,
    detail: &str,
) -> std::fmt::Result {
    write!(
        f,
        "managed task `{task}` process `{process}` is invalid: {detail}"
    )
}

pub(super) fn write_task_managed_profile_tab_order_invalid(
    f: &mut std::fmt::Formatter<'_>,
    task: &str,
    profile: &str,
    detail: &str,
) -> std::fmt::Result {
    write!(
        f,
        "managed task `{task}` profile `{profile}` tab order is invalid: {detail}"
    )
}

pub(super) fn write_task_managed_task_reference_invalid(
    f: &mut std::fmt::Formatter<'_>,
    task: &str,
    process: &str,
    reference: &str,
    detail: &str,
) -> std::fmt::Result {
    write!(
        f,
        "managed task `{task}` process `{process}` task ref `{reference}` is invalid: {detail}"
    )
}

pub(super) fn write_managed_non_zero_exit(
    f: &mut std::fmt::Formatter<'_>,
    task: &str,
    profile: &str,
    processes: &[(String, String)],
) -> std::fmt::Result {
    let rendered = processes
        .iter()
        .map(|(name, diagnostic)| format!("{name} ({diagnostic})"))
        .collect::<Vec<String>>()
        .join(", ");
    write!(
        f,
        "managed task `{task}` profile `{profile}` had non-zero exits: {rendered}"
    )
}
