pub(super) use super::super::prelude::*;

#[derive(Clone, Copy)]
pub(super) enum CatalogResolveFixture {
    RootAndFarmyardApi,
    ManagedProfileInvocation,
}

pub(super) struct CatalogsResolveCase {
    pub(super) workspace: &'static str,
    pub(super) fixture: CatalogResolveFixture,
    pub(super) args: &'static [&'static str],
    pub(super) expected: &'static [&'static str],
}
