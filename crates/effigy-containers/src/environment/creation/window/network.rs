//! Corroborate nerdctl Compose membership when it omits Docker's one-off flag.
use super::*;

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
