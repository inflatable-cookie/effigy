use effigy_cli::{AdmissionArgs, AdmissionSubcommand};

use super::error::RunnerError;

pub(in crate::runner) fn run_admission(args: AdmissionArgs) -> Result<String, RunnerError> {
    let payload = match args.subcommand {
        AdmissionSubcommand::Status => super::admission::status_json(),
        AdmissionSubcommand::Run { run_id } => {
            super::admission::run_json(&run_id).and_then(|record| {
                record.ok_or_else(|| format!("admission run `{run_id}` was not found"))
            })
        }
        AdmissionSubcommand::Runs {
            caller,
            offset,
            limit,
        } => super::admission::runs_json(&caller, offset, limit),
    }
    .map_err(RunnerError::task_invocation)?;

    if args.output_json {
        return Ok(payload);
    }

    let value: serde_json::Value = serde_json::from_str(&payload)
        .map_err(|error| RunnerError::Ui(format!("invalid admission query payload: {error}")))?;
    match value.get("schema").and_then(serde_json::Value::as_str) {
        Some("effigy.admission.status.v1") => Ok(render_status(&value)),
        Some("effigy.admission.run.v1") => Ok(render_run(&value)),
        Some("effigy.admission.runs.v1") => Ok(render_runs(&value)),
        _ => Ok(payload),
    }
}

fn render_status(value: &serde_json::Value) -> String {
    let budget = &value["budget"];
    let mut lines = vec![format!(
        "Host admission budget: {} CPU units, {} MiB",
        budget["cpu_units"].as_u64().unwrap_or_default(),
        budget["memory_mib"].as_u64().unwrap_or_default()
    )];
    for run in value["leases"].as_array().into_iter().flatten() {
        lines.push(format!(
            "{} {} for {} ({})",
            run["state"].as_str().unwrap_or("running"),
            run["selector"].as_str().unwrap_or("<unknown>"),
            run["caller"].as_str().unwrap_or("<unknown>"),
            run["run_id"].as_str().unwrap_or("<unknown>")
        ));
    }
    for run in value["queued"].as_array().into_iter().flatten() {
        lines.push(format!(
            "waiting for QA capacity, position {}: {} ({})",
            run["position"].as_u64().unwrap_or_default(),
            run["selector"].as_str().unwrap_or("<unknown>"),
            run["caller"].as_str().unwrap_or("<unknown>")
        ));
    }
    if lines.len() == 1 {
        lines.push("No heavy validations are waiting or running.".to_owned());
    }
    lines.join("\n")
}

fn render_run(value: &serde_json::Value) -> String {
    format!(
        "{} {} for {} ({})\nqueue wait: {} ms; wall: {} ms; CPU: user {} ms, system {} ms",
        value["state"].as_str().unwrap_or("unknown"),
        value["selector"].as_str().unwrap_or("<unknown>"),
        value["caller"].as_str().unwrap_or("<unknown>"),
        value["run_id"].as_str().unwrap_or("<unknown>"),
        optional_number(&value["queue_wait_ms"]),
        optional_number(&value["wall_ms"]),
        optional_number(&value["cpu_user_ms"]),
        optional_number(&value["cpu_system_ms"]),
    )
}

fn render_runs(value: &serde_json::Value) -> String {
    let caller = value["caller"].as_str().unwrap_or("<unknown>");
    let runs = value["runs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|run| {
            format!(
                "{} {} {} queue={}ms wall={}ms cpu={}+{}ms",
                run["run_id"].as_str().unwrap_or("<unknown>"),
                run["state"].as_str().unwrap_or("unknown"),
                run["selector"].as_str().unwrap_or("<unknown>"),
                optional_number(&run["queue_wait_ms"]),
                optional_number(&run["wall_ms"]),
                optional_number(&run["cpu_user_ms"]),
                optional_number(&run["cpu_system_ms"]),
            )
        })
        .collect::<Vec<_>>();
    if runs.is_empty() {
        format!("No admission history for {caller}.")
    } else {
        runs.join("\n")
    }
}

fn optional_number(value: &serde_json::Value) -> String {
    value
        .as_u64()
        .map(|number| number.to_string())
        .unwrap_or_else(|| "unavailable".to_owned())
}
