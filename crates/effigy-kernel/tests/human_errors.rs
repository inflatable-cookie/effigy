//! Human-facing kernel refusals: what a person reads instead of a `Debug` tree.
//!
//! Every kernel and run error is exactly one of three categories (K4,
//! contract 052 §6): **Refused** (a rule said no), **Unavailable** (something
//! did not answer; retrying is safe) or **Broken** (Effigy's own data is wrong,
//! an Effigy bug). Only verified corruption says `Corrupt` and suggests the
//! explicit repair (effigy#360).
use std::time::Duration;

use effigy_kernel::ctx::{Interrupt, StopCause};
use effigy_kernel::fs::FsError;
use effigy_kernel::human::Category;
use effigy_kernel::observe::{Observed, UnknownReason};
use effigy_kernel::run::RunError;
use effigy_kernel::store::StoreError;

/// The one sentence each category ends with.
fn tail(category: Category) -> String {
    category.next_step().to_owned()
}

#[test]
fn a_busy_store_is_unavailable_and_never_corrupt() {
    // The store waited its whole window and names the holder; nothing on disk
    // is wrong, so this never suggests moving `kernel.db` aside (effigy#360).
    let busy = StoreError::Busy {
        waited: Duration::from_secs(30),
        holder: Box::new(Observed::Unknown(UnknownReason::Busy {
            phase: "store write".into(),
        })),
    };
    let rendered = busy.to_string();
    assert_eq!(
        rendered,
        format!(
            "the state store could not proceed: the state store kernel.db stayed busy for 30.0s \
             while another Effigy process wrote its run journal; {}",
            tail(Category::Unavailable)
        )
    );
    assert_eq!(busy.category(), Category::Unavailable);
    assert!(
        !rendered.contains("corrupt") && !rendered.contains("repair"),
        "{rendered}"
    );
}

#[test]
fn a_kernel_db_that_cannot_be_opened_is_unavailable_and_names_the_path() {
    // `SQLITE_CANTOPEN` is an environment condition: the path or its
    // permissions are wrong, the file is not corrupt (effigy#360).
    let cannot_open = StoreError::from(rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error {
            code: rusqlite::ErrorCode::CannotOpen,
            extended_code: rusqlite::ffi::SQLITE_CANTOPEN,
        },
        Some("unable to open database file".into()),
    ));
    let rendered = cannot_open.to_string();
    assert_eq!(cannot_open.category(), Category::Unavailable);
    // The message names the path it could not open and says retrying is safe.
    assert!(rendered.contains("kernel.db"), "{rendered}");
    assert_eq!(
        rendered,
        format!(
            "the state store could not proceed: sqlite kernel.db: unable to open database file; {}",
            tail(Category::Unavailable)
        )
    );
}

#[test]
fn a_corrupt_journal_says_so_and_names_the_one_repair() {
    // Verified corruption is the only case that says `Corrupt` and suggests
    // the explicit repair (effigy#360).
    let corrupt = StoreError::Corrupt("PRAGMA integrity_check: bad page".into());
    assert_eq!(
        corrupt.to_string(),
        "Effigy is broken: kernel.db is corrupt (PRAGMA integrity_check: bad page); nothing was \
         changed, and the explicit repair moves `kernel.db` and its `-journal`, `-wal` and `-shm` \
         files aside, and then this command works"
    );
    assert_eq!(corrupt.category(), Category::Broken);

    // An on-disk schema older than this binary is corrupt in the same way, and
    // the same repair is the one next step.
    let schema = StoreError::UnknownSchema { found: 0 };
    assert!(schema.to_string().contains("corrupt"), "{schema}");
    assert!(schema.to_string().contains("explicit repair"), "{schema}");
}

#[test]
fn a_journal_from_a_newer_effigy_never_suggests_a_repair() {
    // A schema ahead of this binary is version skew, not damage: the newer
    // Effigy reads the file, and moving it aside would lose a healthy journal
    // (F19).
    let newer = StoreError::UnknownSchema { found: 9 };
    let rendered = newer.to_string();
    assert!(rendered.contains("Effigy was updated"), "{rendered}");
    assert!(rendered.contains("schema version 9"), "{rendered}");
    assert!(!rendered.contains("corrupt"), "{rendered}");
    assert!(!rendered.contains("repair"), "{rendered}");
    assert_eq!(newer.category(), Category::Unavailable);
}

#[test]
fn an_internal_invariant_failure_is_an_effigy_bug_and_names_no_task_list() {
    let broken = RunError::broken("the run's effect ticket belongs to another run");
    let rendered = broken.to_string();
    assert_eq!(
        rendered,
        format!(
            "Effigy is broken: the run's effect ticket belongs to another run; {}",
            tail(Category::Broken)
        )
    );
    assert_eq!(broken.category(), Category::Broken);
    assert!(!rendered.contains("effigy tasks"), "{rendered}");
    assert!(
        !rendered.contains("corrupt") && !rendered.contains("repair"),
        "{rendered}"
    );
    assert!(!rendered.contains("not a valid run request"), "{rendered}");
}

#[test]
fn a_refused_rule_names_the_rule_and_never_the_task_list() {
    let refused = RunError::refused("run 7 still holds task:dev, so stop it first");
    let rendered = refused.to_string();
    assert_eq!(refused.category(), Category::Refused);
    assert!(
        rendered.contains("run 7 still holds task:dev"),
        "{rendered}"
    );
    assert!(!rendered.contains("effigy tasks"), "{rendered}");
    assert!(!rendered.contains("not a valid run request"), "{rendered}");
}

#[test]
fn an_unavailable_step_says_what_was_waited_on_and_that_retrying_is_safe() {
    let unavailable = RunError::step_failed("the state store", "kernel.db was locked");
    let rendered = unavailable.to_string();
    assert_eq!(unavailable.category(), Category::Unavailable);
    assert!(
        rendered.contains("the state store failed (kernel.db was locked)"),
        "{rendered}"
    );
    assert!(rendered.contains("retrying"), "{rendered}");
    assert!(!rendered.contains("Effigy bug"), "{rendered}");
}

#[test]
fn a_settle_timeout_reads_as_unavailable() {
    let refused = RunError::from(StoreError::Interrupted(Interrupt::TimedOut {
        phase: "settle".into(),
        elapsed: Duration::from_millis(247_800),
    }));
    assert_eq!(refused.category(), Category::Unavailable);
    assert_eq!(
        format!("environment record: {refused}"),
        format!(
            "environment record: the state store could not proceed: timed out after 4m08s while \
             waiting for the environment to settle; {}",
            tail(Category::Unavailable)
        )
    );
}

#[test]
fn a_cancelled_run_reads_as_cancelled_and_never_as_refused() {
    // Ctrl-C during a gate wait says what ended the run and asks nothing else:
    // no category's next step applies (F7).
    let cancelled = RunError::from(StoreError::Interrupted(Interrupt::Cancelled {
        cause: StopCause::Signal(libc::SIGINT),
    }));
    assert_eq!(cancelled.category(), Category::Unavailable);
    assert_eq!(
        cancelled.to_string(),
        "the state store was cancelled by SIGINT"
    );
    let gate = effigy_kernel::gate::GateError::Interrupted(Interrupt::Cancelled {
        cause: StopCause::Signal(libc::SIGINT),
    });
    assert_eq!(gate.category(), Category::Unavailable);
    assert_eq!(gate.to_string(), "the gate was cancelled by SIGINT");
    let run = RunError::Interrupted(Interrupt::Cancelled {
        cause: StopCause::Signal(libc::SIGINT),
    });
    assert_eq!(run.to_string(), "the run was cancelled by SIGINT");
    let rendered = run.to_string();
    assert!(
        !rendered.contains("refused") && !rendered.contains("nothing ran"),
        "{rendered}"
    );
}

#[test]
fn a_helper_refusal_prints_the_one_sentence_it_reported() {
    // A supervisor's refusal is already one category with one next step. A
    // parent prints it as it stands instead of framing it again, so a busy
    // store at managed start never reads as a refusal around an unavailable
    // store (effigy#360).
    let refusals = [
        RunError::from(StoreError::Busy {
            waited: Duration::from_secs(30),
            holder: Box::new(Observed::Unknown(UnknownReason::Busy {
                phase: "store write".into(),
            })),
        }),
        RunError::from(StoreError::Corrupt("bad page".into())),
    ];
    for refusal in refusals {
        let sentence = refusal.to_string();
        let reported = RunError::reported(refusal.category(), sentence.clone());
        assert_eq!(reported.to_string(), sentence);
        assert_eq!(reported.category(), refusal.category());
        assert!(sentence.matches("retrying").count() <= 1, "{sentence}");
    }
}

#[test]
fn an_unreadable_store_names_the_store_and_the_next_step() {
    let observed: Observed<&str> = Observed::Unknown(UnknownReason::StoreUnreadable(
        "kernel.db is unreadable (the file is not a database)".to_owned(),
    ));
    assert_eq!(
        format!("checkout scope inventory: {observed}"),
        "checkout scope inventory: could not tell: the store is unreadable \
         (kernel.db is unreadable (the file is not a database)); run `effigy doctor` \
         to inspect the state root"
    );
}

#[test]
fn an_open_state_root_names_the_mode_and_the_chmod() {
    let error = FsError::Untrusted {
        path: "/repo/.effigy/state".into(),
        reason: "group- or other-accessible mode".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "/repo/.effigy/state is readable by other users; run `chmod 700 /repo/.effigy/state`"
    );
    // A permission problem is unavailable, not corruption: the path and the
    // chmod it needs are the whole next step.
    let store = StoreError::from(error);
    assert_eq!(store.category(), Category::Unavailable);
    assert_eq!(
        store.to_string(),
        format!(
            "the state store could not proceed: /repo/.effigy/state is readable by other users, \
             so run `chmod 700 /repo/.effigy/state`; {}",
            tail(Category::Unavailable)
        )
    );
}

#[test]
fn an_untrusted_state_root_names_the_path_and_the_retry() {
    let reason = UnknownReason::Untrusted(
        "/repo/.effigy/state is readable by other users; run `chmod 700 /repo/.effigy/state`"
            .to_owned(),
    );
    assert_eq!(
        reason.to_string(),
        "could not tell: the state root is not trusted (/repo/.effigy/state is readable by other \
         users; run `chmod 700 /repo/.effigy/state`); Effigy only reads state it owns, so fix the \
         mode or ownership it names and retry"
    );
}

/// effigy#508: a backend that keeps no creator record says so, names what it
/// cannot answer, and never reads as corruption or as the absence of an owner.
#[test]
fn a_missing_backend_record_names_the_answer_it_cannot_give() {
    let reason = UnknownReason::NoBackendRecord(
        "Docker Engine keeps no record of which client created a container".to_owned(),
    );
    assert_eq!(
        reason.to_string(),
        "could not tell: the backend keeps no record that answers this (Docker Engine keeps no \
         record of which client created a container); the API it would need does not exist, so a \
         retry cannot change it"
    );
    assert!(!reason.to_string().contains("corrupt"));
}

/// The other honest answer: the backend keeps creator records and none names
/// this container, so no run's create produced it (effigy#508).
#[test]
fn a_creator_record_without_a_row_names_the_container_and_its_project() {
    let absent = effigy_kernel::observe::AbsenceProof::NoCreator {
        creator: effigy_kernel::observe::CreatorRef {
            project: "acme-stack".to_owned(),
            container_id: "3f6a1d0c7b9e".to_owned(),
        },
    };
    assert_eq!(
        absent.to_string(),
        "the backend's creator record names no run for container `3f6a1d0c7b9e` in project \
         `acme-stack`"
    );
}

#[test]
fn a_workload_outcome_says_how_it_ended() {
    use effigy_kernel::outcome::WorkloadOutcome;

    assert_eq!(
        WorkloadOutcome::Succeeded.to_string(),
        "the workload succeeded"
    );
    assert_eq!(
        WorkloadOutcome::Failed { code: 3 }.to_string(),
        "the workload failed with exit code 3"
    );
    assert_eq!(
        WorkloadOutcome::Cancelled {
            cause: StopCause::Signal(libc::SIGINT),
            signal: Some(libc::SIGINT),
        }
        .to_string(),
        "the workload was cancelled by SIGINT"
    );
    assert_eq!(
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        }
        .to_string(),
        "the workload was cancelled by a stop request"
    );
    assert_eq!(
        WorkloadOutcome::TimedOut {
            phase: "settle".into()
        }
        .to_string(),
        "the workload timed out while waiting for the environment to settle"
    );
    assert_eq!(
        WorkloadOutcome::Interrupted.to_string(),
        "the workload was interrupted by a signal"
    );
    assert!(WorkloadOutcome::Unknown(UnknownReason::PermissionDenied)
        .to_string()
        .starts_with("the workload outcome is unknown: could not tell: "));

    for outcome in [
        WorkloadOutcome::TimedOut {
            phase: "settle".into(),
        },
        WorkloadOutcome::Unknown(UnknownReason::GuardUnavailable),
    ] {
        let rendered = outcome.to_string();
        assert!(
            !rendered.contains("TimedOut {") && !rendered.contains("Unknown("),
            "{rendered}"
        );
    }
}

#[test]
fn no_human_rendering_falls_back_to_a_debug_tree() {
    let refusals = [
        Stream::Store(StoreError::UnknownSchema { found: 9 }),
        Stream::Store(StoreError::Corrupt("bad page".into())),
        Stream::Store(StoreError::Broken("a broken check".into())),
        Stream::Store(StoreError::TooLarge { limit: 42 }),
        Stream::Store(StoreError::UnknownResource),
        Stream::Run(RunError::EnvNotActive),
        Stream::Run(RunError::Broken("a broken check".into())),
        Stream::Unknown(UnknownReason::PermissionDenied),
        Stream::Unknown(UnknownReason::GuardUnavailable),
    ];
    for refusal in refusals {
        let rendered = refusal.to_string();
        for debug_only in ["Unknown(", "StoreUnreadable(", "TimedOut {"] {
            assert!(
                !rendered.contains(debug_only),
                "`{rendered}` still carries Debug formatting `{debug_only}`"
            );
        }
    }
}

#[test]
fn every_kernel_error_reads_as_exactly_one_of_the_three_categories() {
    let errors = [
        RunError::EnvNotActive,
        RunError::Broken("a broken check".into()),
        RunError::refused("a rule"),
        RunError::unavailable("a step"),
        RunError::Store(StoreError::Corrupt("bad page".into())),
        RunError::Store(StoreError::Broken("a broken check".into())),
        RunError::from(StoreError::Interrupted(Interrupt::TimedOut {
            phase: "settle".into(),
            elapsed: Duration::from_secs(1),
        })),
    ];
    for error in errors {
        let rendered = error.to_string();
        let frames = ["was refused", "could not proceed", "Effigy is broken"]
            .iter()
            .filter(|frame| rendered.contains(*frame))
            .count();
        assert_eq!(
            frames, 1,
            "`{rendered}` reads as {frames} categories, not one"
        );
    }
}

#[derive(Debug)]
enum Stream {
    Store(StoreError),
    Run(RunError),
    Unknown(UnknownReason),
}
impl std::fmt::Display for Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => error.fmt(f),
            Self::Run(error) => error.fmt(f),
            Self::Unknown(reason) => reason.fmt(f),
        }
    }
}
