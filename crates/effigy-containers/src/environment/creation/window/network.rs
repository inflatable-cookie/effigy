//! Corroborate nerdctl Compose membership when it omits Docker's one-off flag.
use super::*;

#[derive(Debug, Default)]
pub(super) struct Effective {
    networks: serde_yaml::Mapping,
    services: BTreeMap<String, serde_yaml::Mapping>,
}
impl Effective {
    pub(super) fn include(&mut self, document: &serde_yaml::Value) {
        if let Some(networks) = document
            .get("networks")
            .and_then(serde_yaml::Value::as_mapping)
        {
            merge(&mut self.networks, networks);
        }
        if let Some(services) = document
            .get("services")
            .and_then(serde_yaml::Value::as_mapping)
        {
            for (name, body) in services {
                let (Some(name), Some(body)) = (name.as_str(), body.as_mapping()) else {
                    continue;
                };
                let target = self.services.entry(name.into()).or_default();
                if let Some(mode) = body.get("network_mode") {
                    target.insert("network_mode".into(), mode.clone());
                }
                if let Some(networks) = body.get("networks") {
                    // Compose network aliases merge as unique mapping keys,
                    // including the short list spelling. Absent means inherit.
                    let normalized = match networks.as_sequence() {
                        Some(values) => serde_yaml::Value::Mapping(
                            values
                                .iter()
                                .cloned()
                                .map(|name| (name, serde_yaml::Value::Null))
                                .collect(),
                        ),
                        None => networks.clone(),
                    };
                    if let (Some(current), Some(incoming)) = (
                        target
                            .get_mut("networks")
                            .and_then(serde_yaml::Value::as_mapping_mut),
                        normalized.as_mapping(),
                    ) {
                        merge(current, incoming);
                    } else {
                        target.insert("networks".into(), normalized);
                    }
                }
            }
        }
    }
    pub(super) fn for_service(
        &self,
        service: &str,
        project: &str,
    ) -> Result<BTreeSet<String>, UnknownReason> {
        let document = serde_yaml::Value::Mapping(
            [(
                "networks".into(),
                serde_yaml::Value::Mapping(self.networks.clone()),
            )]
            .into_iter()
            .collect(),
        );
        declared(
            &document,
            &self.services.get(service).cloned().unwrap_or_default(),
            project,
        )
    }
}
fn merge(target: &mut serde_yaml::Mapping, incoming: &serde_yaml::Mapping) {
    for (key, value) in incoming {
        if let (Some(current), Some(incoming)) = (
            target
                .get_mut(key)
                .and_then(serde_yaml::Value::as_mapping_mut),
            value.as_mapping(),
        ) {
            merge(current, incoming);
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
}

pub(super) fn declared(
    document: &serde_yaml::Value,
    body: &serde_yaml::Mapping,
    project: &str,
) -> Result<BTreeSet<String>, UnknownReason> {
    if let Some(mode) = body.get("network_mode") {
        let mode = mode
            .as_str()
            .filter(|mode| matches!(*mode, "none" | "host" | "bridge"))
            .ok_or_else(|| {
                inconsistent("unsupported frozen network mode for attribution".into())
            })?;
        return Ok([mode.to_owned()].into());
    }
    let aliases = match body.get("networks") {
        None | Some(serde_yaml::Value::Null) => vec!["default"],
        Some(serde_yaml::Value::Sequence(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| inconsistent("invalid frozen network name".into()))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(serde_yaml::Value::Mapping(values)) => values
            .keys()
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| inconsistent("invalid frozen network name".into()))
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(inconsistent("invalid frozen networks".into())),
    };
    let aliases = if aliases.is_empty() {
        vec!["default"]
    } else {
        aliases
    };
    aliases
        .into_iter()
        .map(|alias| {
            let declaration = document
                .get("networks")
                .and_then(|networks| networks.get(alias));
            let name = declaration
                .and_then(|network| network.get("name"))
                .and_then(|name| name.as_str());
            let external = declaration
                .and_then(|network| network.get("external"))
                .and_then(|value| value.as_bool())
                == Some(true);
            let name = match name {
                Some(name) => name.to_owned(),
                None if external => alias.to_owned(),
                None => format!("{project}_{alias}"),
            };
            if name.is_empty() || name.contains('$') {
                return Err(inconsistent(
                    "frozen network name is empty or unresolved".into(),
                ));
            }
            Ok(name)
        })
        .collect()
}

pub(super) fn corroborates(row: &Workload, expected: &BTreeSet<String>) -> bool {
    if row.labels.get("nerdctl/name") != Some(&row.name) {
        return false;
    }
    let Some(networks) = row.labels.get("nerdctl/networks") else {
        return false;
    };
    let Ok(networks) = serde_json::from_str::<Vec<String>>(networks) else {
        return false;
    };
    let actual: BTreeSet<_> = networks.iter().cloned().collect();
    !actual.is_empty() && actual.len() == networks.len() && &actual == expected
}
