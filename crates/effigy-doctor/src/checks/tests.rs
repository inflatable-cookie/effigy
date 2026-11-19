use super::definitions::doctor_check_definitions;

/// The built-in check registry is a stable ordered surface: the run order and
/// the progress labels downstream surfaces parse must not drift silently.
#[test]
fn doctor_check_registry_order_and_progress_labels_are_stable() {
    let order = doctor_check_definitions()
        .iter()
        .map(|check| check.name)
        .collect::<Vec<&str>>();
    assert_eq!(
        order,
        vec![
            "manifest_conflicts",
            "environment_tools",
            "task_references",
            "pinned_sources",
            "god_files",
            "duplicate_blocks",
            "health_task",
        ]
    );

    let labels = doctor_check_definitions()
        .iter()
        .filter_map(|check| check.progress_label.map(|label| label.active))
        .collect::<Vec<&str>>();
    assert_eq!(
        labels,
        vec![
            "scanning for god files",
            "scanning for duplicate blocks",
            "running health task",
        ]
    );
}

/// A walk that ran out of budget keeps the completed evidence of the health
/// task that ran beside it, and reports that task with its own state.
#[test]
fn health_that_finished_beside_a_budget_exhausted_walk_keeps_its_evidence() {
    let mut state = crate::DoctorState::new();
    state.record_budget_exhausted("scan_inventory", std::time::Duration::ZERO);
    let run = super::HealthRun {
        plan: crate::health::HealthPlan::Run {
            health_catalogs: vec!["app".to_owned()],
        },
        label: None,
        outcome: Some(Ok(
            r#"{"schema":"effigy.task.run.v1","stdout":"healthy","stderr":"","exit_code":0}"#
                .to_owned(),
        )),
        elapsed: std::time::Duration::from_millis(5),
        stopped: false,
    };
    let findings_before = state.findings.len();

    super::finish_health_check(run, &mut state, None);

    assert!(state.findings.len() > findings_before);
    assert!(matches!(
        state
            .check_runs
            .last()
            .map(|run| (&run.name[..], &run.state)),
        Some(("health_task", crate::report::DoctorCheckRunState::Complete))
    ));
    assert!(!state.is_complete());
}

/// A health task the shared deadline cut off is reported as budget-exhausted.
#[test]
fn health_cut_off_by_the_deadline_is_reported_as_budget_exhausted() {
    let mut state = crate::DoctorState::new();
    let run = super::HealthRun {
        plan: crate::health::HealthPlan::Run {
            health_catalogs: vec!["app".to_owned()],
        },
        label: None,
        outcome: Some(Err(crate::DoctorError::BudgetExhausted {
            phase: "health_task".to_owned(),
        })),
        elapsed: std::time::Duration::from_millis(5),
        stopped: true,
    };

    super::finish_health_check(run, &mut state, None);

    assert!(matches!(
        state
            .check_runs
            .last()
            .map(|run| (&run.name[..], &run.state)),
        Some((
            "health_task",
            crate::report::DoctorCheckRunState::BudgetExhausted
        ))
    ));
    assert!(!state.is_complete());
}
