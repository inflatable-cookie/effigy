use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use super::ast::{EnvSchema, EnvSchemaEntry, EnvValueExpr, TemplatePart};
use super::error::EnvSchemaError;
use super::exec::run_exec_command;
use super::secret::{ResolvedValue, SecretString};

/// Configuration for environment resolution.
pub struct ResolutionContext {
    pub process_env: BTreeMap<String, String>,
    pub dotenv_overrides: BTreeMap<String, String>,
    pub exec_timeout: Duration,
    pub project_root: PathBuf,
}

/// The result of resolving an env schema — all entries with their values.
#[derive(Debug)]
pub struct ResolvedEnv {
    entries: BTreeMap<String, ResolvedEntry>,
}

/// A single resolved environment entry.
#[derive(Debug)]
pub struct ResolvedEntry {
    pub value: ResolvedValue,
    pub source: ResolvedSource,
}

/// Where a resolved value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedSource {
    ProcessEnv,
    DotenvOverride,
    SchemaDefault,
    ExecCommand,
    EnvReference,
    TemplateInterpolation,
}

impl ResolvedEnv {
    /// Return all non-sensitive entries as a plain key-value map.
    pub fn plain_env(&self) -> BTreeMap<String, String> {
        self.entries
            .iter()
            .filter(|(_, entry)| !entry.value.is_secret())
            .map(|(key, entry)| (key.clone(), entry.value.as_str().to_owned()))
            .collect()
    }

    /// Return references to all sensitive entries.
    pub fn secret_env(&self) -> Vec<(&str, &SecretString)> {
        self.entries
            .iter()
            .filter_map(|(key, entry)| match &entry.value {
                ResolvedValue::Secret(s) => Some((key.as_str(), s)),
                ResolvedValue::Plain(_) => None,
            })
            .collect()
    }

    /// Check if a key exists in the resolved environment.
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Get the value for a key (as &str, exposing secrets if needed).
    pub fn get_value(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|e| e.value.as_str())
    }

    /// Get the entry for a key.
    pub fn get(&self, key: &str) -> Option<&ResolvedEntry> {
        self.entries.get(key)
    }

    /// Number of resolved entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Resolve all entries in a schema against the given context.
pub fn resolve_schema(
    schema: &EnvSchema,
    context: &ResolutionContext,
) -> Result<ResolvedEnv, EnvSchemaError> {
    let resolution_order = compute_resolution_order(schema)?;
    let mut resolved = BTreeMap::new();

    for key in &resolution_order {
        let entry = schema
            .entries
            .iter()
            .find(|e| &e.key == key)
            .expect("resolution order contains only schema keys");
        let resolved_entry = resolve_entry(entry, context, &resolved)?;
        if let Some(re) = resolved_entry {
            resolved.insert(key.clone(), re);
        } else if entry.annotations.required == Some(true) {
            return Err(EnvSchemaError::MissingRequired {
                key: key.clone(),
                line: entry.line,
            });
        }
    }

    Ok(ResolvedEnv { entries: resolved })
}

fn resolve_entry(
    entry: &EnvSchemaEntry,
    context: &ResolutionContext,
    already_resolved: &BTreeMap<String, ResolvedEntry>,
) -> Result<Option<ResolvedEntry>, EnvSchemaError> {
    // Priority 1: process environment
    if let Some(value) = context.process_env.get(&entry.key) {
        return Ok(Some(ResolvedEntry {
            value: wrap_value(value.clone(), entry.annotations.sensitive),
            source: ResolvedSource::ProcessEnv,
        }));
    }

    // Priority 2: .env file overrides
    if let Some(value) = context.dotenv_overrides.get(&entry.key) {
        return Ok(Some(ResolvedEntry {
            value: wrap_value(value.clone(), entry.annotations.sensitive),
            source: ResolvedSource::DotenvOverride,
        }));
    }

    // Priority 3: schema value expression
    match &entry.value {
        None => Ok(None),
        Some(expr) => resolve_value_expr(expr, entry, context, already_resolved),
    }
}

fn resolve_value_expr(
    expr: &EnvValueExpr,
    entry: &EnvSchemaEntry,
    context: &ResolutionContext,
    already_resolved: &BTreeMap<String, ResolvedEntry>,
) -> Result<Option<ResolvedEntry>, EnvSchemaError> {
    match expr {
        EnvValueExpr::Literal(value) => Ok(Some(ResolvedEntry {
            value: wrap_value(value.clone(), entry.annotations.sensitive),
            source: ResolvedSource::SchemaDefault,
        })),

        EnvValueExpr::Exec(command) => {
            let output = run_exec_command(command, context.exec_timeout, &context.project_root)?;
            Ok(Some(ResolvedEntry {
                value: wrap_value(output, entry.annotations.sensitive),
                source: ResolvedSource::ExecCommand,
            }))
        }

        EnvValueExpr::EnvRef(var_name) => {
            let value = lookup_var(var_name, context, already_resolved).ok_or_else(|| {
                EnvSchemaError::Resolution {
                    key: entry.key.clone(),
                    message: format!("referenced variable `{var_name}` has no value"),
                }
            })?;
            Ok(Some(ResolvedEntry {
                value: wrap_value(value, entry.annotations.sensitive),
                source: ResolvedSource::EnvReference,
            }))
        }

        EnvValueExpr::Template(parts) => {
            let mut result = String::new();
            for part in parts {
                match part {
                    TemplatePart::Literal(text) => result.push_str(text),
                    TemplatePart::VarRef(var_name) => {
                        let value =
                            lookup_var(var_name, context, already_resolved).ok_or_else(|| {
                                EnvSchemaError::Resolution {
                                    key: entry.key.clone(),
                                    message: format!("template variable `{var_name}` has no value"),
                                }
                            })?;
                        result.push_str(&value);
                    }
                }
            }
            Ok(Some(ResolvedEntry {
                value: wrap_value(result, entry.annotations.sensitive),
                source: ResolvedSource::TemplateInterpolation,
            }))
        }
    }
}

fn lookup_var(
    name: &str,
    context: &ResolutionContext,
    already_resolved: &BTreeMap<String, ResolvedEntry>,
) -> Option<String> {
    // Check already-resolved schema entries first
    if let Some(entry) = already_resolved.get(name) {
        return Some(entry.value.as_str().to_owned());
    }
    // Then process env
    if let Some(value) = context.process_env.get(name) {
        return Some(value.clone());
    }
    // Then dotenv overrides
    if let Some(value) = context.dotenv_overrides.get(name) {
        return Some(value.clone());
    }
    None
}

fn wrap_value(value: String, sensitive: bool) -> ResolvedValue {
    if sensitive {
        ResolvedValue::Secret(SecretString::new(value))
    } else {
        ResolvedValue::Plain(value)
    }
}

/// Compute the order in which entries should be resolved, detecting cycles.
///
/// Entries that depend on other schema entries (via `env()` or `${VAR}`)
/// must be resolved after their dependencies.
fn compute_resolution_order(schema: &EnvSchema) -> Result<Vec<String>, EnvSchemaError> {
    let schema_keys: HashSet<&str> = schema.entries.iter().map(|e| e.key.as_str()).collect();

    // Build adjacency list: key -> set of keys it depends on (within schema)
    let mut deps: HashMap<&str, Vec<&str>> = HashMap::new();
    for entry in &schema.entries {
        let mut entry_deps = Vec::new();
        if let Some(ref expr) = entry.value {
            collect_dependencies(expr, &schema_keys, &mut entry_deps);
        }
        deps.insert(&entry.key, entry_deps);
    }

    // Topological sort via DFS
    let mut order = Vec::new();
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    for entry in &schema.entries {
        if !visited.contains(entry.key.as_str()) {
            topo_visit(&entry.key, &deps, &mut visited, &mut in_stack, &mut order)?;
        }
    }

    Ok(order)
}

fn collect_dependencies<'a>(
    expr: &'a EnvValueExpr,
    schema_keys: &HashSet<&str>,
    out: &mut Vec<&'a str>,
) {
    match expr {
        EnvValueExpr::Literal(_) | EnvValueExpr::Exec(_) => {}
        EnvValueExpr::EnvRef(var) => {
            if schema_keys.contains(var.as_str()) {
                out.push(var);
            }
        }
        EnvValueExpr::Template(parts) => {
            for part in parts {
                if let TemplatePart::VarRef(var) = part {
                    if schema_keys.contains(var.as_str()) {
                        out.push(var);
                    }
                }
            }
        }
    }
}

fn topo_visit<'a>(
    key: &'a str,
    deps: &HashMap<&str, Vec<&'a str>>,
    visited: &mut HashSet<&'a str>,
    in_stack: &mut HashSet<&'a str>,
    order: &mut Vec<String>,
) -> Result<(), EnvSchemaError> {
    if in_stack.contains(key) {
        let chain: Vec<String> = in_stack.iter().map(|s| (*s).to_owned()).collect();
        let mut chain = chain;
        chain.push(key.to_owned());
        return Err(EnvSchemaError::CircularDependency { chain });
    }
    if visited.contains(key) {
        return Ok(());
    }

    in_stack.insert(key);
    if let Some(key_deps) = deps.get(key) {
        for dep in key_deps {
            topo_visit(dep, deps, visited, in_stack, order)?;
        }
    }
    in_stack.remove(key);
    visited.insert(key);
    order.push(key.to_owned());
    Ok(())
}

#[cfg(test)]
#[path = "resolver/tests.rs"]
mod tests;
