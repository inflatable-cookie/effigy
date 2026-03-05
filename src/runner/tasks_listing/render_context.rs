use crate::TasksArgs;

#[derive(Clone, Copy)]
pub(super) enum ListingSelection<'a> {
    Filtered(&'a str),
    Catalog,
}

#[derive(Clone, Copy)]
pub(super) enum ListingOutputMode {
    Json,
    Text,
}

#[derive(Clone, Copy)]
pub(super) enum TextRenderDispatch<'a> {
    ResolveOnly(&'a serde_json::Value),
    Filtered(&'a str),
    Catalog,
}

#[derive(Clone, Copy)]
pub(super) struct ListingRenderRequest<'a> {
    output_mode: ListingOutputMode,
    selection: ListingSelection<'a>,
    resolve_probe: &'a Option<serde_json::Value>,
    resolve_only_probe: Option<&'a serde_json::Value>,
    pretty_json: bool,
}

impl ListingOutputMode {
    fn from_args(args: &TasksArgs) -> Self {
        if args.output_json {
            return Self::Json;
        }
        Self::Text
    }
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
            output_mode: ListingOutputMode::from_args(args),
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

    pub(super) fn output_mode(self) -> ListingOutputMode {
        self.output_mode
    }

    pub(super) fn resolve_probe(&self) -> &'a Option<serde_json::Value> {
        self.resolve_probe
    }

    pub(super) fn pretty_json(&self) -> bool {
        self.pretty_json
    }

    pub(super) fn dispatch_selection<R>(
        self,
        on_filtered: impl FnOnce(&'a str) -> R,
        on_catalog: impl FnOnce() -> R,
    ) -> R {
        match self.selection {
            ListingSelection::Filtered(filter) => on_filtered(filter),
            ListingSelection::Catalog => on_catalog(),
        }
    }

    pub(super) fn text_dispatch(self) -> TextRenderDispatch<'a> {
        if let Some(probe) = self.resolve_only_probe {
            return TextRenderDispatch::ResolveOnly(probe);
        }
        match self.selection {
            ListingSelection::Filtered(filter) => TextRenderDispatch::Filtered(filter),
            ListingSelection::Catalog => TextRenderDispatch::Catalog,
        }
    }
}
