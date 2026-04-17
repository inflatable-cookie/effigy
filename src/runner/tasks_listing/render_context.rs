use effigy_cli::TasksArgs;

#[derive(Clone, Copy)]
pub(super) enum ListingSelection<'a> {
    Filtered(&'a str),
    Catalog,
}

#[derive(Clone, Copy)]
pub(super) struct ListingRenderRequest<'a> {
    output_json: bool,
    selection: ListingSelection<'a>,
    resolve_probe: &'a Option<serde_json::Value>,
    resolve_only_probe: Option<&'a serde_json::Value>,
    pretty_json: bool,
}

impl<'a> ListingRenderRequest<'a> {
    pub(super) fn from_args(
        args: &'a TasksArgs,
        resolve_probe: &'a Option<serde_json::Value>,
    ) -> Self {
        let filter = args.task_name.as_deref();
        let selection = match filter {
            Some(filter) => ListingSelection::Filtered(filter),
            None => ListingSelection::Catalog,
        };
        Self {
            output_json: args.output_json,
            selection,
            resolve_probe,
            resolve_only_probe: if filter.is_none() {
                resolve_probe.as_ref()
            } else {
                None
            },
            pretty_json: args.pretty_json,
        }
    }

    pub(super) fn output_json(self) -> bool {
        self.output_json
    }

    pub(super) fn resolve_probe(&self) -> &'a Option<serde_json::Value> {
        self.resolve_probe
    }

    pub(super) fn pretty_json(&self) -> bool {
        self.pretty_json
    }

    pub(super) fn selection(self) -> ListingSelection<'a> {
        self.selection
    }

    pub(super) fn resolve_only_probe(self) -> Option<&'a serde_json::Value> {
        self.resolve_only_probe
    }
}
