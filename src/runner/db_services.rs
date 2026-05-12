use std::collections::BTreeMap;

use effigy_data::{
    collect_database_services_from_manifest_entries, DatabaseService, DatabaseServiceManifestEntry,
};
use effigy_manifest::ManifestContainerServiceConfig;

pub(in crate::runner) fn collect_manifest_database_services(
    services: &BTreeMap<String, ManifestContainerServiceConfig>,
) -> Vec<DatabaseService> {
    let entries = services
        .iter()
        .map(|(service_name, service)| {
            DatabaseServiceManifestEntry::new(service_name.clone(), service.catalog.clone())
                .password(service_string_param(service, "password"))
                .declared_databases(service_string_array_param(service, "databases"))
                .primary_database(service_string_param(service, "database"))
        })
        .collect::<Vec<_>>();
    collect_database_services_from_manifest_entries(&entries)
}

fn service_string_param(service: &ManifestContainerServiceConfig, key: &str) -> Option<String> {
    service
        .params
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
}

fn service_string_array_param(service: &ManifestContainerServiceConfig, key: &str) -> Vec<String> {
    service
        .params
        .get(key)
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_data::DatabaseServiceKind;

    #[test]
    fn collects_manifest_database_services_from_container_service_params() {
        let mut services = BTreeMap::new();
        services.insert(
            "mysql".to_owned(),
            ManifestContainerServiceConfig {
                catalog: "mysql".to_owned(),
                variant: None,
                config: None,
                shared: None,
                params: BTreeMap::from([
                    (
                        "password".to_owned(),
                        toml::Value::String(" mysql-secret ".to_owned()),
                    ),
                    (
                        "databases".to_owned(),
                        toml::Value::Array(vec![
                            toml::Value::String(" legacy ".to_owned()),
                            toml::Value::String(" ".to_owned()),
                        ]),
                    ),
                    (
                        "database".to_owned(),
                        toml::Value::String(" legacy ".to_owned()),
                    ),
                ]),
            },
        );
        services.insert(
            "redis".to_owned(),
            ManifestContainerServiceConfig {
                catalog: "redis".to_owned(),
                variant: None,
                config: None,
                shared: None,
                params: BTreeMap::new(),
            },
        );

        let collected = collect_manifest_database_services(&services);

        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].name, "mysql");
        assert_eq!(collected[0].kind, DatabaseServiceKind::MariaDb);
        assert_eq!(collected[0].password, "mysql-secret");
        assert_eq!(collected[0].declared_databases, vec!["legacy".to_owned()]);
        assert_eq!(collected[0].primary_database.as_deref(), Some("legacy"));
    }
}
