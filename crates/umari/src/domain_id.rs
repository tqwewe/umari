use indexmap::IndexMap;

use crate::error::FromDomainIdsError;

/// Domain ID bindings from a command input.
///
/// Maps domain ID field names to the values to query for.
/// Multiple input fields can map to the same domain ID field name.
pub type DomainIdBindings = IndexMap<&'static str, String>;

pub trait DomainIds {
    /// The domain id fields.
    const DOMAIN_ID_FIELDS: &'static [&'static str];

    fn domain_ids(&self) -> DomainIdBindings;
}

pub trait FromDomainIds: Sized {
    type Args;

    fn from_domain_ids(
        args: Self::Args,
        bindings: &DomainIdBindings,
    ) -> Result<Self, FromDomainIdsError>;
}
