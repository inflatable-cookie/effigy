const SECTION_DEFER: &[&str] = &[
    "[defer]",
    "# Fallback command for unresolved task requests.",
    "run = \"my-process {request} {args}\"",
    "# Optional built-in command families that should bypass Effigy's built-in implementation.",
    "builtins = [\"release\"]",
    "",
];

const SECTION_SCAN: &[&str] = &[
    "[scan.god_files]",
    "# Oversized code-file scanner thresholds use code-only lines.",
    "# Without explicit thresholds each file uses its language defaults:",
    "# typescript/javascript/vue/svelte/python/ruby/php: 800/1400/2400;",
    "# rust/c/c++/c#/go/java/kotlin/scala/swift: 1000/1800/3000;",
    "# sql/css/html: 1200/2000/3200; shell/lua: 500/900/1500.",
    "warn = 1000",
    "high = 1800",
    "critical = 3000",
    "# Include this scanner in `effigy doctor` by default.",
    "doctor = true",
    "fail_on_findings = false",
    "respect_gitignore = true",
    "# Optional glob overrides.",
    "include = [\"src/**\", \"app/**\"]",
    "exclude = [\"docs/**\", \"dist/**\", \"coverage/**\"]",
    "",
    "[scan.duplicate_blocks]",
    "# Duplicate-block scanner thresholds use normalized code-only line spans.",
    "warn = 20",
    "high = 40",
    "critical = 80",
    "min_occurrences = 2",
    "# Keep this opt-in for `effigy doctor` until runtime/noise is validated.",
    "doctor = false",
    "fail_on_findings = false",
    "respect_gitignore = true",
    "# Optional glob overrides.",
    "include = [\"src/**\", \"crates/**\", \"tests/**\"]",
    "exclude = [\"vendor/**\"]",
    "",
];

const SECTION_TASKS_MINIMAL: &[&str] = &[
    "[tasks]",
    "# Compact task command mappings.",
    "test = \"bun x vitest run\"",
    "",
];

pub(super) fn defer_lines() -> &'static [&'static str] {
    SECTION_DEFER
}

pub(super) fn scan_lines() -> &'static [&'static str] {
    SECTION_SCAN
}

pub(super) fn tasks_minimal_lines() -> &'static [&'static str] {
    SECTION_TASKS_MINIMAL
}
