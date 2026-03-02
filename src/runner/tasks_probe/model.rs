use serde_json::json;

pub(super) struct ResolveProbe {
    selector: String,
    status: &'static str,
    catalog: Option<String>,
    catalog_root: Option<String>,
    task: String,
    lock_scopes: Vec<String>,
    evidence: Vec<String>,
    error: Option<String>,
}

impl ResolveProbe {
    pub(super) fn ok(
        selector: &str,
        task: &str,
        catalog: Option<String>,
        catalog_root: Option<String>,
        lock_scopes: Vec<String>,
        evidence: Vec<String>,
    ) -> Self {
        Self {
            selector: selector.to_owned(),
            status: "ok",
            catalog,
            catalog_root,
            task: task.to_owned(),
            lock_scopes,
            evidence,
            error: None,
        }
    }

    pub(super) fn error(
        selector: &str,
        task: &str,
        lock_scopes: Vec<String>,
        error: String,
    ) -> Self {
        Self {
            selector: selector.to_owned(),
            status: "error",
            catalog: None,
            catalog_root: None,
            task: task.to_owned(),
            lock_scopes,
            evidence: Vec::new(),
            error: Some(error),
        }
    }

    pub(super) fn into_json(self) -> serde_json::Value {
        json!({
            "selector": self.selector,
            "status": self.status,
            "catalog": self.catalog,
            "catalog_root": self.catalog_root,
            "task": self.task,
            "lock_scopes": self.lock_scopes,
            "evidence": self.evidence,
            "error": self.error,
        })
    }
}
