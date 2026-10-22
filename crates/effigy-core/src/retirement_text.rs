//! Retirement guidance shared by the surfaces that refuse a removed feature.
//!
//! One literal per retirement keeps the manifest refusal, the built-in command
//! and the CLI parser from drifting apart.

/// The Q-009 demo-to-task conversion recipe (Tom, 2026-10-22).
///
/// Shared by the `[demos]` manifest refusal, `effigy config --target demos` and
/// `effigy demo`, so every demo retirement gives the same next step.
pub const DEMO_TO_TASK_RECIPE: &str = "each `[demos.<name>]` becomes a plain `[tasks.\"demo:<name>\"]` with the same `run`: before, `[demos.checkout]` with `title = \"Checkout\"`, `summary = \"...\"` and `run = \"bun run demo\"`; after, `[tasks.\"demo:checkout\"]` with that `run`, `title` and `summary` kept as a TOML comment (tasks have no description field), and `group = \"Demos\"` to keep the demos grouped";

#[cfg(test)]
mod tests {
    use super::DEMO_TO_TASK_RECIPE;

    #[test]
    fn retirement_demo_recipe_names_the_task_shape_and_the_optional_group() {
        assert!(DEMO_TO_TASK_RECIPE.contains("`[tasks.\"demo:<name>\"]`"));
        assert!(DEMO_TO_TASK_RECIPE.contains("`[demos.checkout]`"));
        assert!(DEMO_TO_TASK_RECIPE.contains("`[tasks.\"demo:checkout\"]`"));
        assert!(DEMO_TO_TASK_RECIPE.contains("as a TOML comment"));
        assert!(DEMO_TO_TASK_RECIPE.contains("group = \"Demos\""));
    }
}
