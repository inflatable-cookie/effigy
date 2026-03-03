pub(super) use super::super::super::prelude::*;

pub(super) fn write_ranked_task_ref_manifest(root: &PathBuf, jobs_start_after_ms: Option<u32>) {
    let jobs_delay = jobs_start_after_ms
        .map(|ms| format!(", start_after_ms = {ms}"))
        .unwrap_or_default();
    write_root_manifest(
        root,
        &format!(
            r#"[tasks.dev]
mode = "tui"
concurrent = [
  {{ task = "farmyard/api", start = 1, tab = 3 }},
  {{ task = "farmyard/jobs", start = 2, tab = 4{} }},
  {{ task = "cream/dev", start = 3, tab = 2 }},
  {{ task = "dairy/dev", start = 4, tab = 1 }}
]
"#,
            jobs_delay
        ),
    );
}

pub(super) fn write_ranked_name_manifest(root: &PathBuf) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api", start = 1, tab = 3 },
  { name = "jobs", run = "printf jobs", start = 2, tab = 4 },
  { name = "cream", run = "printf cream", start = 3, tab = 2 },
  { name = "dairy", run = "printf dairy", start = 4, tab = 1 }
]
"#,
    );
}

pub(super) fn write_ranked_catalog_tasks(root: &PathBuf) {
    write_catalogs_with_tasks(
        root,
        &[
            (
                "farmyard",
                &[("api", "printf farmyard-api"), ("jobs", "printf farmyard-jobs")],
            ),
            ("cream", &[("dev", "printf cream-dev")]),
            ("dairy", &[("dev", "printf dairy-dev")]),
        ],
    );
}
