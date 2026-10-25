//! The runtime restarts a stopped service.
//!
//! A rendered Compose file asks the runtime itself to bring a service back
//! after a VM restart, a Colima restart or a crash, so Effigy never hand-starts
//! containers. Every service without its own `restart:` gets
//! [`RESTART_POLICY`]; a service that declares one keeps it byte for byte.

use serde_yaml::Value;

/// The restart policy every generated service gets unless it declares one.
pub(crate) const RESTART_POLICY: &str = "unless-stopped";

const RESTART_KEY: &str = "restart";

/// Add `restart: unless-stopped` to every service that does not declare a
/// restart policy of its own. A declared policy (`no`, `always`, `on-failure`,
/// …) wins, including an explicit `no`.
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
        let document = rendered("services:\n  web:\n    image: nginx\n  db:\n    image: mysql\n");
        for service in ["web", "db"] {
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
