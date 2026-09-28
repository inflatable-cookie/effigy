use crate::help::{HelpRenderer, HelpResult, TableSpec};

pub(crate) fn render_admission_help(renderer: &mut dyn HelpRenderer) -> HelpResult<()> {
    renderer.text("Inspect the shared host budget used by heavy Effigy validation tasks.")?;
    renderer.section("Commands")?;
    renderer.table(&TableSpec {
        headers: vec!["Command".to_owned(), "Purpose".to_owned()],
        rows: vec![
            vec![
                "effigy admission status --json".to_owned(),
                "Show the live budget, waiting positions, and leases".to_owned(),
            ],
            vec![
                "effigy admission run <RUN_ID> --json".to_owned(),
                "Show one durable caller-linked run record".to_owned(),
            ],
            vec![
                "effigy admission runs --caller <ID> --json".to_owned(),
                "Show bounded caller history; supports --offset and --limit".to_owned(),
            ],
        ],
    })?;
    renderer.section("Settings")?;
    renderer.table(&TableSpec {
        headers: vec!["Environment variable".to_owned(), "Meaning".to_owned()],
        rows: vec![
            vec![
                "EFFIGY_CALLER".to_owned(),
                "Caller identity recorded with each run".to_owned(),
            ],
            vec![
                "EFFIGY_ADMISSION_DIR".to_owned(),
                "Private host state directory (default: ~/.cache/effigy/admission)".to_owned(),
            ],
            vec![
                "EFFIGY_ADMISSION_TIMEOUT_SECS".to_owned(),
                "Capacity wait deadline (default: 1800 seconds)".to_owned(),
            ],
            vec![
                "EFFIGY_ADMISSION_CPU_BUDGET".to_owned(),
                "Host CPU reservation budget".to_owned(),
            ],
            vec![
                "EFFIGY_ADMISSION_MEMORY_BUDGET_MIB".to_owned(),
                "Host memory reservation budget in MiB".to_owned(),
            ],
            vec![
                "EFFIGY_ADMISSION_CPU_UNITS".to_owned(),
                "Per-run CPU reservation".to_owned(),
            ],
            vec![
                "EFFIGY_ADMISSION_MEMORY_MIB".to_owned(),
                "Per-run memory reservation in MiB".to_owned(),
            ],
        ],
    })?;
    renderer.text("Reservations coordinate concurrency; they are not OS CPU or memory limits.")?;
    Ok(())
}
