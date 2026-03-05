use super::super::{LoadedCatalog, RunnerError};
use super::filtering::{evaluate_task_filter, FilteredTaskModel};
use super::render_context::{ListingRenderRequest, ListingSelection};
use super::ListingCatalogSnapshot;

pub(super) enum PreparedListingSelection<'req, 'snap> {
    Catalog {
        ordered_catalogs: &'snap [&'snap LoadedCatalog],
    },
    Filtered {
        filter: &'req str,
        filtered_model: FilteredTaskModel<'snap>,
    },
}

pub(super) fn prepare_listing_selection<'req, 'snap>(
    request: ListingRenderRequest<'req>,
    snapshot: &'snap ListingCatalogSnapshot<'snap>,
) -> Result<PreparedListingSelection<'req, 'snap>, RunnerError> {
    match request.selection() {
        ListingSelection::Catalog => Ok(PreparedListingSelection::Catalog {
            ordered_catalogs: snapshot.ordered_catalogs(),
        }),
        ListingSelection::Filtered(filter) => Ok(PreparedListingSelection::Filtered {
            filter,
            filtered_model: evaluate_task_filter(snapshot.catalogs(), filter)?,
        }),
    }
}
