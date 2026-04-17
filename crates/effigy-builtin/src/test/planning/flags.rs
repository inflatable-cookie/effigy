use super::model::BuiltinTestCliFlags;

pub(super) fn extract_builtin_test_flags(
    raw_args: &[String],
) -> (BuiltinTestCliFlags, Vec<String>) {
    let mut flags = BuiltinTestCliFlags {
        plan_mode: false,
        verbose_results: false,
        tui: false,
        output_json: false,
    };
    let mut passthrough = Vec::<String>::new();
    let mut in_passthrough = false;
    for arg in raw_args {
        if in_passthrough {
            passthrough.push(arg.clone());
            continue;
        }

        if arg == "--" {
            in_passthrough = true;
        } else if arg == "--plan" {
            flags.plan_mode = true;
        } else if arg == "--verbose-results" {
            flags.verbose_results = true;
        } else if arg == "--tui" {
            flags.tui = true;
        } else if arg == "--json" {
            flags.output_json = true;
        } else {
            passthrough.push(arg.clone());
        }
    }
    (flags, passthrough)
}

#[cfg(test)]
#[path = "flags/tests.rs"]
mod tests;
