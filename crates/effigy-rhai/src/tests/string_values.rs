//! String values across helper boundaries (papercut eb53ef2b).
//!
//! Rhai's built-in string methods mutate the receiver and return `()`:
//! `text.replace(needle, substitute)`, `text.trim()`, and friends. The reported
//! defect blamed the optimizer for turning strings into unit when a variable
//! needle or a loop variable was involved. These regressions pin the real rule
//! so a future author meets it in a test rather than in a checker failure:
//! mutating methods always return unit, whatever the needle, position, or
//! optimization level, and Effigy's value-returning `str::*` helpers are the
//! safe pattern at every boundary.

use super::*;

/// A pure-Rhai script (no Effigy host functions) so the same source can run
/// under several optimizer levels.
const PURE_SCRIPT: &str = r#"
fn mutating_statement(text) {
    text.replace("-", "_");
}
fn count_unit(values) {
    let units = 0;
    for value in values {
        if value == "()" {
            units += 1;
        }
    }
    units
}
let variable = "-";
let observed = [];
for needle in [variable, "-"] {
    observed.push(type_of("a-b".replace(needle, "_")));
}
observed.push(type_of(mutating_statement("a-b")));
observed.push(type_of("a-b".trim()));
count_unit(observed)
"#;

#[test]
fn mutating_string_methods_return_unit_at_every_optimization_level() {
    let mut expectations = Vec::new();
    for level in [
        rhai::OptimizationLevel::None,
        rhai::OptimizationLevel::Simple,
        rhai::OptimizationLevel::Full,
    ] {
        let mut engine = crate::configured_rhai_engine();
        engine.set_optimization_level(level);
        let units: i64 = engine.eval(PURE_SCRIPT).expect("pure script runs");
        expectations.push((level, units));
    }

    for (level, units) in expectations {
        assert_eq!(
            units, 4,
            "two literal/variable needles, a statement-position call and trim each return unit at this level: mutating `replace`/`trim` must return unit for literal and variable needles at {level:?}"
        );
    }
}

#[test]
fn value_returning_string_helpers_survive_functions_loops_and_statement_position() {
    let root = temp_root("string-value-helpers");
    let script = r#"
fn normalise(text, needle) {
    str::replace(text, needle, "_")
}
fn normalise_each(text, needles) {
    let rendered = [];
    for needle in needles {
        rendered.push(str::replace(text, needle, "_"));
    }
    rendered
}
let needle = "-";
if normalise("a-b", needle) != "a_b" {
    throw("str::replace must return a value from a function body");
}
if normalise_each("a-b", [needle, "."]) != ["a_b", "a-b"] {
    throw("str::replace must return a value from a loop");
}
str::replace("a-b", needle, "_");
if str::contains("a-b", needle) != true {
    throw("statement-position str helpers must not poison the next string operation");
}
if "a-b".replace(needle, "_") + "!" != "!" {
    throw("Rhai's mutating replace returns unit, which concatenates silently");
}
"#;

    execute_rhai_script(&script_context(&root), script, &[], &callbacks()).expect("execute");
}
