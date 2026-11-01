use super::*;
use effigy_containers::environment::{BackendSelection, EnvironmentBackend, Workload};
use std::path::Path;
use std::time::{Duration, Instant};

struct Backend {
    selected: BackendSelection,
    rows: Vec<Workload>,
    before: bool,
}
impl EnvironmentBackend for Backend {
    fn selection(&self) -> &BackendSelection {
        &self.selected
    }
    fn inventory(&mut self, _: &Path, _: &str) -> Observed<Vec<Workload>> {
        panic!("unbounded inventory")
    }
    fn inventory_until(&mut self, _: &Path, _: &str, deadline: Instant) -> Observed<Vec<Workload>> {
        assert!(Instant::now() < deadline);
        let rows = if std::mem::take(&mut self.before) {
            vec![]
        } else {
            self.rows.clone()
        };
        verified(rows)
    }
    fn image_id_until(&mut self, _: &Path, _: &str, deadline: Instant) -> Observed<String> {
        assert!(Instant::now() < deadline);
        verified(format!("sha256:{}", "cc".repeat(32)))
    }
}
fn verified<T>(value: T) -> Observed<T> {
    Observed::Verified(
        value,
        effigy_kernel::observe::Evidence::new("creator fixture".into(), None).unwrap(),
    )
}

#[test]
fn an_ordinary_apply_cannot_supply_reserved_creator_attribution() {
    use effigy_containers::environment::{EnvRun, Operation, Retention, Up};
    use effigy_kernel::gate::Wait;
    let fixture = effigy_test_support::Fixture::new("creator-adapter-").unwrap();
    let checkout = fixture.path().join("checkout");
    let home = fixture.path().join("home/.effigy");
    std::fs::create_dir_all(checkout.join(".git")).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(checkout.join("effigy.toml"), "[containers]\ndefault=\"one\"\n[containers.one]\ncompose_file=\"compose.yml\"\nprimary_service=\"app\"\n").unwrap();
    std::fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n",
    )
    .unwrap();
    effigy_containers::with_test_effigy_home(&home, || {
        let policy = effigy_containers::load_container_policy(&checkout, None).unwrap();
        let ctx = crate::runner::environment::context(&checkout).unwrap();
        let env = crate::runner::environment::env_ref(&policy).unwrap();
        let mut labels: std::collections::BTreeMap<String, String> = [
            ("com.effigy.managed", "true"),
            ("com.effigy.project", &policy.project_name),
            ("com.effigy.scope", "scope"),
            ("com.effigy.checkout", checkout.to_str().unwrap()),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect();
        let closure = effigy_kernel::objects::Closure::new([("compose.yml".into(), serde_json::to_vec(&serde_json::json!({"services":{"app":{"image":"example:one","labels":labels}}})).unwrap())].into()).unwrap();
        labels.insert(
            "com.docker.compose.project".into(),
            policy.project_name.clone(),
        );
        labels.insert("com.docker.compose.service".into(), "app".into());
        labels.insert("com.docker.compose.oneoff".into(), "False".into());
        let id = "aa".repeat(32);
        let mut backend = Backend {
            selected: BackendSelection {
                kind: effigy_containers::environment::BackendKind::DockerDesktop,
                engine: effigy_containers::BackendId::docker_compose(),
                context: "fixture".into(),
                reason: effigy_containers::environment::BackendReason::ManifestDriver(
                    "docker".into(),
                ),
            },
            before: true,
            rows: vec![Workload {
                id: id.clone(),
                name: "app-1".into(),
                running: true,
                labels,
                binds: vec![],
                image: format!("sha256:{}", "cc".repeat(32)),
                image_reference: "example:one".into(),
            }],
        };
        let mut run = EnvRun::begin(&ctx, &env, Operation::Up, Wait::FailFast).unwrap();
        let selection = backend.selected.clone();
        let request = Up {
            render: Some(&closure),
            project: &policy.project_name,
            backend: &selection,
            retention: Retention::Persistent,
            workdir: &checkout,
            primary_service: "app",
            compose_files: &[],
        }
        .creation_request();
        let outcome = run
            .create_with(&mut backend, &selection, request, || Ok(()))
            .unwrap();
        assert!(
            outcome.receipt().is_none(),
            "ordinary applies grant no attribution"
        );
        let journal = WorkspaceCreatorEvidence::new(&ctx, env.resource.clone());
        let deadline = PublicationDeadline::new(Instant::now() + Duration::from_secs(5));
        assert!(!matches!(
            journal.created_by(&policy.project_name, &id, deadline),
            Observed::Verified(..)
        ));
        assert!(!matches!(
            journal.created_by(&policy.project_name, &"bb".repeat(32), deadline),
            Observed::Verified(..)
        ));
        assert!(!matches!(
            journal.created_by("foreign", &id, deadline),
            Observed::Verified(..)
        ));
        assert!(matches!(
            journal.created_by(
                &policy.project_name,
                &id,
                PublicationDeadline::new(Instant::now() - Duration::from_secs(1))
            ),
            Observed::Unknown(UnknownReason::TimedOut { .. })
        ));
        run.finish(false).unwrap();
    });
}
