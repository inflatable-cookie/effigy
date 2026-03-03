pub(super) use super::super::prelude::*;

pub(super) struct ConfigErrorCase {
    pub(super) workspace: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) expected: &'static [&'static str],
}
