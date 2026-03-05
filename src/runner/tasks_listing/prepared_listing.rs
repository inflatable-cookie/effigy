use super::super::{LoadedCatalog, RunnerError};
use super::filtering::{prepare_filtered_listing, PreparedFilteredListing};
use super::render_context::{ListingRenderRequest, ListingSelection};
use super::ListingCatalogSnapshot;

pub(super) enum PreparedListingSelection<'snap> {
    Catalog {
        ordered_catalogs: &'snap [&'snap LoadedCatalog],
    },
    Filtered {
        filtered_listing: PreparedFilteredListing<'snap>,
    },
}

pub(super) fn prepare_listing_selection<'snap>(
    request: ListingRenderRequest<'_>,
    snapshot: &'snap ListingCatalogSnapshot<'snap>,
) -> Result<PreparedListingSelection<'snap>, RunnerError> {
    match request.selection() {
        ListingSelection::Catalog => Ok(PreparedListingSelection::Catalog {
            ordered_catalogs: snapshot.ordered_catalogs(),
        }),
        ListingSelection::Filtered(filter) => Ok(PreparedListingSelection::Filtered {
            filtered_listing: prepare_filtered_listing(snapshot.catalogs(), filter)?,
        }),
    }
}
