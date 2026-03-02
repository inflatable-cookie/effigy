use crate::TasksArgs;

pub(super) enum TextRenderMode<'a> {
    Filtered(&'a str),
    ResolveOnly(&'a serde_json::Value),
    Catalog,
}

pub(super) struct ListingRenderContext<'a> {
    filter: Option<&'a str>,
    resolve_probe: &'a Option<serde_json::Value>,
    pretty_json: bool,
}

impl<'a> ListingRenderContext<'a> {
    pub(super) fn new(args: &'a TasksArgs, resolve_probe: &'a Option<serde_json::Value>) -> Self {
        Self {
            filter: args.task_name.as_deref(),
            resolve_probe,
            pretty_json: args.pretty_json,
        }
    }

    pub(super) fn filter(&self) -> Option<&'a str> {
        self.filter
    }

    pub(super) fn resolve_probe(&self) -> &'a Option<serde_json::Value> {
        self.resolve_probe
    }

    pub(super) fn pretty_json(&self) -> bool {
        self.pretty_json
    }

    pub(super) fn text_mode(&self) -> TextRenderMode<'a> {
        if let Some(filter) = self.filter {
            return TextRenderMode::Filtered(filter);
        }
        if let Some(probe) = self.resolve_probe.as_ref() {
            return TextRenderMode::ResolveOnly(probe);
        }
        TextRenderMode::Catalog
    }
}
