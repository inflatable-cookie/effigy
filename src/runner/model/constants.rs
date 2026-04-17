pub(in crate::runner) const DEFER_DEPTH_ENV: &str = "EFFIGY_DEFER_DEPTH";
pub(in crate::runner) const IMPLICIT_ROOT_DEFER_TEMPLATE: &str =
    "{composer_global_effigy} {request} {args}";
pub(in crate::runner) const EXPLICITLY_DEFERRABLE_COMMAND_BUILTINS: [&str; 7] = [
    "changelog",
    "docs",
    "contracts",
    "distribution",
    "release",
    "doctor",
    "tasks",
];
pub(in crate::runner) const IMPLICITLY_DEFERRED_COMMAND_BUILTINS: [&str; 1] = ["release"];

// `BUILTIN_TASKS` and `DEFAULT_BUILTIN_TEST_MAX_PARALLEL` moved into
// `effigy_builtin::constants` under card 250. Runner-side consumers
// (`tasks_probe::resolve`, `tasks_listing::row_projection`) import
// `effigy_builtin::BUILTIN_TASKS` directly.
