use super::super::model::catalog::LoadedCatalog;
use super::render_context::ListingRenderRequest;
use super::selection::{
    prepare_listing_selection, PreparedFilteredListing, PreparedListingSelection,
};
use super::ListingCatalogSnapshot;
use crate::runner::error::RunnerError;

pub(super) fn dispatch_listing_selection<'snap, Ctx, T>(
    request: ListingRenderRequest<'_>,
    snapshot: &'snap ListingCatalogSnapshot<'snap>,
    context: &mut Ctx,
    on_catalog: impl FnOnce(&mut Ctx, &[&'snap LoadedCatalog]) -> Result<T, RunnerError>,
    on_filtered: impl FnOnce(&mut Ctx, PreparedFilteredListing<'snap>) -> Result<T, RunnerError>,
) -> Result<T, RunnerError> {
    match prepare_listing_selection(request, snapshot)? {
        PreparedListingSelection::Filtered { filtered_listing } => {
            on_filtered(context, filtered_listing)
        }
        PreparedListingSelection::Catalog { ordered_catalogs } => {
            on_catalog(context, ordered_catalogs)
        }
    }
}
