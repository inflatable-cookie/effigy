use super::super::super::util::shell_quote;
use super::{RunStepPolicy, DEFAULT_DAG_MAX_PARALLEL};

pub(super) fn render_parallel_run_levels_with_policy(
    commands: &[String],
    levels: &[Vec<usize>],
    policies: &[RunStepPolicy],
) -> String {
    let max_parallel = dag_max_parallel();
    let mut lines = Vec::<String>::new();
    lines.push("__effigy_overall_status=0".to_owned());

    for level in levels {
        for batch in level.chunks(max_parallel) {
            append_spawn_lines(&mut lines, commands, policies, batch);
            append_wait_lines(&mut lines, policies, batch);
        }
    }

    lines.push("exit \"$__effigy_overall_status\"".to_owned());
    lines.join("\n")
}

fn append_spawn_lines(
    lines: &mut Vec<String>,
    commands: &[String],
    policies: &[RunStepPolicy],
    batch: &[usize],
) {
    for (offset, index) in batch.iter().enumerate() {
        lines.push(format!(
            "({}) & __effigy_pid_{}=$!",
            render_policy_wrapped_command(&commands[*index], policies[*index]),
            offset + 1
        ));
    }
}

fn append_wait_lines(lines: &mut Vec<String>, policies: &[RunStepPolicy], batch: &[usize]) {
    for (offset, index) in batch.iter().enumerate() {
        lines.push(format!("wait \"$__effigy_pid_{}\"", offset + 1));
        lines.push("__effigy_status=$?".to_owned());
        lines.push("if [ \"$__effigy_status\" -ne 0 ]; then".to_owned());
        append_failure_policy_lines(lines, policies[*index]);
        lines.push("fi".to_owned());
    }
}

fn append_failure_policy_lines(lines: &mut Vec<String>, policy: RunStepPolicy) {
    if policy.fail_fast {
        lines.push("  exit \"$__effigy_status\"".to_owned());
    } else {
        lines.push("  __effigy_overall_status=1".to_owned());
    }
}

fn dag_max_parallel() -> usize {
    std::env::var("EFFIGY_DAG_MAX_PARALLEL")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_DAG_MAX_PARALLEL)
}

fn render_policy_wrapped_command(command: &str, policy: RunStepPolicy) -> String {
    let mut lines = Vec::<String>::new();
    lines.push("__effigy_attempt=0".to_owned());
    lines.push("while :".to_owned());
    lines.push("do".to_owned());
    lines.push(render_wrapped_exec_line(command, policy.timeout_ms));
    lines.push("  __effigy_status=$?".to_owned());
    lines.push("  if [ \"$__effigy_status\" -eq 0 ]; then".to_owned());
    lines.push("    break".to_owned());
    lines.push("  fi".to_owned());
    lines.push(format!(
        "  if [ \"$__effigy_attempt\" -ge {} ]; then",
        policy.retry
    ));
    lines.push("    break".to_owned());
    lines.push("  fi".to_owned());
    lines.push("  __effigy_attempt=$((__effigy_attempt + 1))".to_owned());
    if policy.retry_delay_ms > 0 {
        lines.push(format!(
            "  sleep {}",
            (policy.retry_delay_ms as f64) / 1000.0_f64
        ));
    }
    lines.push("done".to_owned());
    lines.push("exit \"$__effigy_status\"".to_owned());
    format!("sh -lc {}", shell_quote(&lines.join("\n")))
}

fn render_wrapped_exec_line(command: &str, timeout_ms: Option<u64>) -> String {
    let quoted_command = shell_quote(command);
    if let Some(timeout_ms) = timeout_ms {
        return format!(
            "  python3 -c 'import subprocess,sys\ntry:\n r=subprocess.run([\"sh\",\"-lc\",sys.argv[2]], timeout=float(sys.argv[1]))\n sys.exit(r.returncode)\nexcept subprocess.TimeoutExpired:\n sys.exit(124)' {} {}",
            (timeout_ms as f64) / 1000.0_f64,
            quoted_command
        );
    }
    format!("  sh -lc {}", quoted_command)
}
