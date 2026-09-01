use super::*;
use crate::{configured_rhai_engine, RHAI_MAX_EXPR_DEPTH, RHAI_MAX_FUNCTION_EXPR_DEPTH};

/// Left-associative `0 + 1 + ... + N` inside a function body.
///
/// Empirically, under Rhai 1.x this is accepted at `N=11` and rejected at
/// `N=12` when function expression depth is `16`, and accepted at `N=27` /
/// rejected at `N=28` when the depth is `32`.
fn function_add_chain(terms: usize) -> String {
    let mut script = String::from("fn f() {\n    let x = 0");
    for i in 1..=terms {
        script.push_str(&format!(" + {i}"));
    }
    script.push_str(";\n    x\n}\nf();\n");
    script
}

#[test]
fn configured_engine_reports_profile_independent_expression_limits() {
    let engine = configured_rhai_engine();
    assert_eq!(engine.max_expr_depth(), RHAI_MAX_EXPR_DEPTH);
    assert_eq!(
        engine.max_function_expr_depth(),
        RHAI_MAX_FUNCTION_EXPR_DEPTH
    );
    assert_eq!(RHAI_MAX_EXPR_DEPTH, 64);
    assert_eq!(RHAI_MAX_FUNCTION_EXPR_DEPTH, 32);
}

#[test]
fn function_expression_above_stock_debug_limit_runs_on_configured_engine() {
    // Exceeds Rhai's stock debug function depth (16) while staying inside
    // Effigy's explicit release envelope (32).
    let script = function_add_chain(20);
    #[cfg(debug_assertions)]
    {
        let raw = rhai::Engine::new();
        assert_eq!(raw.max_function_expr_depth(), 16);
        assert!(
            raw.compile(&script).is_err(),
            "fixture must exceed the stock debug default so the regression is non-vacuous"
        );
    }

    let root = temp_root("expr-within-32");
    execute_rhai_script(&script_context(&root), &script, &[], &callbacks())
        .expect("configured host must accept a function expression above stock debug depth");
}

#[test]
fn function_expression_above_effigy_limit_is_rejected() {
    let script = function_add_chain(40);
    let engine = configured_rhai_engine();
    let error = engine
        .compile(&script)
        .expect_err("over-limit fixture must fail under Effigy's finite guard");
    assert!(
        error
            .to_string()
            .contains("Expression exceeds maximum complexity"),
        "unexpected error: {error}"
    );

    let root = temp_root("expr-over-32");
    let runtime_error = execute_rhai_script(&script_context(&root), &script, &[], &callbacks())
        .expect_err("configured host must keep the finite upper bound");
    assert!(
        runtime_error
            .to_string()
            .contains("Expression exceeds maximum complexity"),
        "unexpected error: {runtime_error}"
    );
}

#[test]
fn first_party_rhai_scripts_compile_on_configured_engine() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf();
    let engine = configured_rhai_engine();
    let mut failures = Vec::new();
    for script in collect_rhai_scripts(&repo_root) {
        let contents = fs::read_to_string(&script).expect("read script");
        if let Err(error) = engine.compile(&contents) {
            failures.push(format!(
                "{}: {error}",
                script.strip_prefix(&repo_root).unwrap_or(&script).display()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "first-party Rhai scripts must compile under Effigy's explicit expression limits:\n{}",
        failures.join("\n")
    );
}
