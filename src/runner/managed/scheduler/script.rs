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
            for (offset, index) in batch.iter().enumerate() {
                lines.push(format!(
                    "({}) & __effigy_pid_{}=$!",
                    render_policy_wrapped_command(&commands[*index], policies[*index]),
                    offset + 1
                ));
            }
            for (offset, index) in batch.iter().enumerate() {
                lines.push(format!("wait \"$__effigy_pid_{}\"", offset + 1));
                lines.push("__effigy_status=$?".to_owned());
                lines.push("if [ \"$__effigy_status\" -ne 0 ]; then".to_owned());
                if policies[*index].fail_fast {
                    lines.push("  exit \"$__effigy_status\"".to_owned());
                } else {
                    lines.push("  __effigy_overall_status=1".to_owned());
                }
                lines.push("fi".to_owned());
            }
        }
    }

    lines.push("exit \"$__effigy_overall_status\"".to_owned());
    lines.join("\n")
}

fn dag_max_parallel() -> usize {
    std::env::var("EFFIGY_DAG_MAX_PARALLEL")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_DAG_MAX_PARALLEL)
}

fn render_policy_wrapped_command(command: &str, policy: RunStepPolicy) -> String {
    let timeout_secs = policy
        .timeout_ms
        .map_or(0.0_f64, |value| (value as f64) / 1000.0_f64);
    let retry_delay_secs = (policy.retry_delay_ms as f64) / 1000.0_f64;
    let mut lines = Vec::<String>::new();
    lines.push("__effigy_attempt=0".to_owned());
    lines.push("while :".to_owned());
    lines.push("do".to_owned());
    if policy.timeout_ms.is_some() {
        lines.push(format!(
            "  python3 -c 'import subprocess,sys\ntry:\n r=subprocess.run([\"sh\",\"-lc\",sys.argv[2]], timeout=float(sys.argv[1]))\n sys.exit(r.returncode)\nexcept subprocess.TimeoutExpired:\n sys.exit(124)' {} {}",
            timeout_secs,
            shell_quote(command)
        ));
    } else {
        lines.push(format!("  sh -lc {}", shell_quote(command)));
    }
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
        lines.push(format!("  sleep {}", retry_delay_secs));
    }
    lines.push("done".to_owned());
    lines.push("exit \"$__effigy_status\"".to_owned());
    format!("sh -lc {}", shell_quote(&lines.join("\n")))
}
