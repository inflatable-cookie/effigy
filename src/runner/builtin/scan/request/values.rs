use crate::runner::error::RunnerError;

pub(super) fn parse_positive_usize_flag(
    flag: &str,
    raw: Option<&str>,
) -> Result<Option<usize>, RunnerError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value = raw.parse::<usize>().map_err(|_| {
        RunnerError::task_invocation(format!(
            "invalid `{flag}` value `{raw}` (expected an integer >= 1)"
        ))
    })?;
    if value == 0 {
        return Err(RunnerError::task_invocation(format!(
            "invalid `{flag}` value `{raw}` (expected an integer >= 1)"
        )));
    }
    Ok(Some(value))
}

pub(super) fn parse_positive_f64_flag(
    flag: &str,
    raw: Option<&str>,
) -> Result<Option<f64>, RunnerError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value = raw.parse::<f64>().map_err(|_| {
        RunnerError::task_invocation(format!(
            "invalid `{flag}` value `{raw}` (expected a number > 0)"
        ))
    })?;
    if value <= 0.0 {
        return Err(RunnerError::task_invocation(format!(
            "invalid `{flag}` value `{raw}` (expected a number > 0)"
        )));
    }
    Ok(Some(value))
}
