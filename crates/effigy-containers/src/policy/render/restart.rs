//! The runtime restarts a stopped service.
//!
//! A rendered Compose file asks the runtime itself to bring a service back
//! after a VM restart, a Colima restart or a crash, so Effigy never hand-starts
//! containers. Every service that defines itself (`image`, `build` or
//! `extends`) and declares no `restart:` gets [`RESTART_POLICY`]; a declared
//! policy is byte for byte what the stack wrote, and an overlay file (a user
//! override, or Effigy's runtime DNS override) never gains one, because the
//! later `-f` file would beat the file that defines the service (effigy#371).

use serde_yaml::Value;

/// The restart policy every generated service gets unless it declares one.
pub(crate) const RESTART_POLICY: &str = "unless-stopped";

const RESTART_KEY: &str = "restart";
/// A service definition names an image, a build, or the fragment it extends.
/// An overlay names none of them, so it only overrides a service it never
/// defines.
const DEFINING_KEYS: [&str; 3] = ["image", "build", "extends"];

/// Add `restart: unless-stopped` to every service that defines itself and does
/// not declare a restart policy of its own. A declared policy (`no`, `always`,
/// `on-failure`, …) wins, including an explicit `no`; an overlay file is left
/// untouched.
pub(crate) fn apply(document: &mut Value) {
    let Some(services) = document.get_mut("services").and_then(Value::as_mapping_mut) else {
        return;
    };
    let key = Value::String(RESTART_KEY.into());
    let policy = Value::String(RESTART_POLICY.into());
    for service in services.values_mut() {
        let Some(service) = service.as_mapping_mut() else {
            continue;
        };
        if service.contains_key(&key) {
            continue;
        }
        if !DEFINING_KEYS
            .iter()
            .any(|defining| service.contains_key(Value::String((*defining).into())))
        {
            continue;
        }
        service.insert(key.clone(), policy.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(document: &str) -> Value {
        let mut document: Value = serde_yaml::from_str(document).expect("yaml");
        apply(&mut document);
        document
    }

    #[test]
    fn every_service_without_a_policy_gets_unless_stopped() {
        let document = rendered(
            "services:\n  web:\n    image: nginx\n  db:\n    build: .\n  ext:\n    extends: base\n",
        );
        for service in ["web", "db", "ext"] {
            assert_eq!(
                document["services"][service]["restart"],
                Value::String(RESTART_POLICY.into()),
                "{service} is restarted by the runtime"
            );
        }
    }

    #[test]
    fn a_declared_policy_wins() {
        let document = rendered(
            "services:\n  web:\n    image: nginx\n    restart: \"no\"\n  db:\n    image: mysql\n    restart: on-failure\n",
        );
        assert_eq!(
            document["services"]["web"]["restart"],
            Value::String("no".into())
        );
        assert_eq!(
            document["services"]["db"]["restart"],
            Value::String("on-failure".into())
        );
    }

    #[test]
    fn an_overlay_never_gains_a_policy_of_its_own() {
        let document = rendered("services:\n  web:\n    dns:\n      - 1.1.1.1\n");
        assert_eq!(
            document["services"]["web"],
            serde_yaml::from_str::<Value>("dns:\n  - 1.1.1.1\n").expect("yaml"),
            "the later -f file would beat the service's own file"
        );
    }

    #[test]
    fn a_document_without_services_is_untouched() {
        let mut document: Value = serde_yaml::from_str("volumes:\n  data: {}\n").expect("yaml");
        apply(&mut document);
        assert_eq!(
            document,
            serde_yaml::from_str::<Value>("volumes:\n  data: {}\n").expect("yaml")
        );
    }

    #[test]
    fn applying_twice_changes_nothing() {
        let once = rendered("services:\n  web:\n    image: nginx\n");
        let mut twice = once.clone();
        apply(&mut twice);
        assert_eq!(once, twice);
    }
}
