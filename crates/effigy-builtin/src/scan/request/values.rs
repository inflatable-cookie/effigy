use crate::BuiltinError;

pub(super) fn parse_positive_usize_flag(
    flag: &str,
    raw: Option<&str>,
) -> Result<Option<usize>, BuiltinError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value = raw.parse::<usize>().map_err(|_| {
        BuiltinError::task_invocation(format!(
            "invalid `{flag}` value `{raw}` (expected an integer >= 1)"
        ))
    })?;
    if value == 0 {
        return Err(BuiltinError::task_invocation(format!(
            "invalid `{flag}` value `{raw}` (expected an integer >= 1)"
        )));
    }
    Ok(Some(value))
}
