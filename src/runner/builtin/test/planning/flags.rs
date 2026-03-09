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
mod tests {
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
}
