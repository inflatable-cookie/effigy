use super::*;

#[test]
fn run_manifest_task_builtin_scan_dead_code_reports_isolated_and_unreferenced_findings() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-findings",
        Some(
            r#"[scan.dead_code]
doctor = false
allow_paths = ["src/bin/**"]
"#,
        ),
        &["src/bin", "src/dead", "src/live", "src/orphan"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\npub mod orphan;\n").expect("write lib");
    fs::write(
        root.join("src/dead/mod.rs"),
        "fn unused_file() -> usize { 0 }\n",
    )
    .expect("write dead");
    fs::write(
        root.join("src/live/mod.rs"),
        "use crate::orphan::helper;\npub fn used() -> usize { helper() }\n",
    )
    .expect("write live");
    fs::write(
        root.join("src/orphan/mod.rs"),
        "fn lonely() -> usize { 1 }\npub fn helper() -> usize { 2 }\n",
    )
    .expect("write orphan");
    fs::write(
        root.join("src/bin/tool.rs"),
        "pub fn main() -> usize { 1 }\n",
    )
    .expect("write bin");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_contains_all(
        &out,
        &[
            "Dead Code",
            "isolated-file",
            "src/dead/mod.rs",
            "unreferenced-symbol",
            "lonely (function)",
        ],
    );
    assert_output_excludes_all(&out, &["helper (function)", "src/bin/tool.rs"]);
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_refuses_stale_index() {
    // Regression for g08.016: a stale graph index reports drifted symbol
    // positions and missing edges, which surface as false-positive dead-code
    // findings. The scan must refuse on a stale index and direct the operator
    // to refresh, rather than presenting stale results as authoritative.
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-stale-index",
        Some("[scan.dead_code]\ndoctor = false\n"),
        &["src/live"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        "pub fn lonely() -> usize { 1 }\n",
    )
    .expect("write live");
    seed_graph_index(&root);

    // Mutate a source file after indexing so the graph index goes stale.
    fs::write(
        root.join("src/live/mod.rs"),
        "pub fn lonely() -> usize { 1 }\npub fn added_after_index() -> usize { 2 }\n",
    )
    .expect("rewrite live");

    let error = run_builtin_err(root, "scan", &["dead-code"]);
    let rendered = error.to_string();
    assert!(
        rendered.contains("requires a fresh graph index"),
        "stale index should be refused with remediation, got: {rendered}"
    );
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_respects_symbol_allowlist() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-allow-symbol",
        Some(
            r#"[scan.dead_code]
doctor = false
allow_symbols = ["crate::live::lonely"]
"#,
        ),
        &["src/live"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        "pub fn lonely() -> usize { 1 }\n",
    )
    .expect("write live");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_excludes_all(&out, &["lonely (function)"]);
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_ignores_public_rust_api_roots() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-public-api",
        Some(
            r#"[scan.dead_code]
doctor = false
"#,
        ),
        &["src/api"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod api;\n").expect("write lib");
    fs::write(
        root.join("src/api/mod.rs"),
        "pub struct PublicApi;\nfn private_helper() -> usize { 1 }\n",
    )
    .expect("write api");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_excludes_all(&out, &["PublicApi (struct)"]);
    assert_output_contains_all(&out, &["private_helper (function)"]);
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_ignores_rust_test_entrypoints_only() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-test-scope",
        Some(
            r#"[scan.dead_code]
doctor = false
"#,
        ),
        &["src/live"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        r#"
fn private_helper() -> usize { 1 }

#[cfg(test)]
mod tests {
    fn used_fixture_helper() -> usize { 1 }
    fn unused_fixture_helper() -> usize { 2 }

    #[test]
    fn private_test_case() {
        assert_eq!(used_fixture_helper(), 1);
    }
}
"#,
    )
    .expect("write live");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_contains_all(
        &out,
        &["private_helper (function)", "unused_fixture_helper"],
    );
    assert_output_excludes_all(&out, &["private_test_case"]);
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_ignores_trait_surface_methods_only() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-trait-surface",
        Some(
            r#"[scan.dead_code]
doctor = false
"#,
        ),
        &["src/live"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        r#"
pub trait JobRunner {
    fn run_job(&self) -> usize;
}

struct LocalRunner;

impl JobRunner for LocalRunner {
    fn run_job(&self) -> usize { 1 }
}

impl LocalRunner {
    fn private_helper(&self) -> usize { 2 }
}
"#,
    )
    .expect("write live");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_contains_all(&out, &["private_helper (function)"]);
    assert_output_excludes_all(
        &out,
        &[
            "run_job (function)",
            "run_job (method)",
            "JobRunner (trait)",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_ignores_generic_trait_impl_methods_only() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-generic-trait-impl",
        Some(
            r#"[scan.dead_code]
doctor = false
"#,
        ),
        &["src/live"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        r#"
pub trait Sink {
    fn write(&mut self, body: &str) -> usize;
}

struct BufferedSink<W> {
    inner: W,
}

impl<W> Sink for BufferedSink<W> {
    fn write(&mut self, body: &str) -> usize { body.len() }
}

impl<W> BufferedSink<W> {
    fn private_helper(&self) -> usize { 2 }
}
"#,
    )
    .expect("write live");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_contains_all(&out, &["private_helper (function)"]);
    assert_output_excludes_all(
        &out,
        &["write (function)", "write (method)", "Sink (trait)"],
    );
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_ignores_descriptor_dispatch_roots_only() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-descriptor-roots",
        Some(
            r#"[scan.dead_code]
doctor = false
"#,
        ),
        &["src/live"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        r#"
struct Descriptor {
    render: fn() -> usize,
}

const DESCRIPTORS: &[Descriptor] = &[
    Descriptor {
        render: render_from_descriptor,
    },
];

const DISPATCH: &[fn() -> usize] = &[dispatch_from_table];

fn render_from_descriptor() -> usize { 1 }
fn dispatch_from_table() -> usize { 2 }
fn private_helper() -> usize { 3 }
"#,
    )
    .expect("write live");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_contains_all(&out, &["private_helper (function)"]);
    assert_output_excludes_all(
        &out,
        &[
            "render_from_descriptor (function)",
            "dispatch_from_table (function)",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_resolves_self_method_calls() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-self-method-calls",
        Some(
            r#"[scan.dead_code]
doctor = false
"#,
        ),
        &["src/live"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        r#"
pub struct Worker;

impl Worker {
    pub fn run(&self) -> usize {
        self.prepare()
    }

    fn prepare(&self) -> usize { 1 }

    fn private_helper(&self) -> usize { 2 }
}
"#,
    )
    .expect("write live");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_contains_all(&out, &["private_helper (function)"]);
    assert_output_excludes_all(&out, &["prepare (function)"]);
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_ignores_macro_and_argument_function_refs_only() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-function-ref-roots",
        Some(
            r#"[scan.dead_code]
doctor = false
"#,
        ),
        &["src/live"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        r#"
fn macro_called() -> &'static str { "ready" }
fn mapped_value(value: usize) -> usize { value + 1 }
fn private_helper() -> usize { 3 }

pub fn render() -> Vec<usize> {
    let labels = vec![macro_called()];
    labels.iter().map(|_| 1).map(mapped_value).collect()
}
"#,
    )
    .expect("write live");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_contains_all(&out, &["private_helper (function)"]);
    assert_output_excludes_all(
        &out,
        &["macro_called (function)", "mapped_value (function)"],
    );
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_ignores_entrypoints_and_attribute_roots() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-entrypoints-and-attributes",
        Some(
            r#"[scan.dead_code]
doctor = false
"#,
        ),
        &["src/bin", "src/live"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\n").expect("write lib");
    fs::write(root.join("src/bin/tool.rs"), "fn main() {}\n").expect("write bin");
    fs::write(
        root.join("src/live/mod.rs"),
        r#"
use serde::Deserialize;

trait LocalValidator {
    fn validate(&self) -> bool;
}

trait MultiLineTrait:
    LocalValidator
{
    fn apply(&self) -> bool;
}

struct Rule;

impl LocalValidator for Rule {
    fn validate(&self) -> bool { true }
}

impl MultiLineTrait for Rule {
    fn apply(&self) -> bool { self.validate() }
}

#[derive(Deserialize)]
struct Config {
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_enabled() -> bool { true }
fn private_helper() -> usize { 1 }
"#,
    )
    .expect("write live");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_contains_all(&out, &["private_helper (function)"]);
    assert_output_excludes_all(
        &out,
        &[
            "main (function)",
            "LocalValidator (trait)",
            "MultiLineTrait (trait)",
            "validate (function)",
            "apply (function)",
            "apply (method)",
            "default_enabled (function)",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_ignores_crate_src_modules_and_extension_traits() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-crate-src-modules",
        Some(
            r#"[scan.dead_code]
doctor = false
"#,
        ),
        &["crates/member/src/inner", "src/live"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\n").expect("write root lib");
    fs::write(
        root.join("src/live/mod.rs"),
        r#"
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T { f(self) }
}

impl<T> Pipe for T {}

pub fn use_pipe() -> usize {
    1usize.pipe(|value| value + 1)
}

fn private_helper() -> usize { 1 }
"#,
    )
    .expect("write live");
    fs::write(root.join("crates/member/src/lib.rs"), "pub mod inner;\n").expect("write member lib");
    fs::write(
        root.join("crates/member/src/inner/mod.rs"),
        "pub fn declared_module() -> usize { 1 }\n",
    )
    .expect("write inner module");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_contains_all(&out, &["private_helper (function)"]);
    assert_output_excludes_all(&out, &["Pipe (trait)", "crates/member/src/inner/mod.rs"]);
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_ignores_referenced_data_shapes_only() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-data-shapes",
        Some(
            r#"[scan.dead_code]
doctor = false
"#,
        ),
        &["src/live"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        r#"
struct RenderPayload {
    row: RenderRow,
}

enum RenderRow {
    Text(String),
}

struct UnusedPayload {
    value: String,
}

fn render_payload() -> RenderPayload {
    RenderPayload {
        row: RenderRow::Text("ready".to_owned()),
    }
}
"#,
    )
    .expect("write live");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_contains_all(
        &out,
        &["UnusedPayload (struct)", "render_payload (function)"],
    );
    assert_output_excludes_all(&out, &["RenderPayload (struct)", "RenderRow (enum)"]);
}
