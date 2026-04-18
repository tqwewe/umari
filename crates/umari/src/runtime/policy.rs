use std::{cell::RefCell, marker::PhantomData};

pub use self::exports::umari::policy::policy::{CommandSubmission, Guest, GuestPolicy};
use crate::{
    policy::Policy,
    runtime::common::{self, EventQuery, StoredEvent},
};

wit_bindgen::generate!({
    path: "wit/policy",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    pub_export_macro: true,
    with: {
        "umari:common/types@0.1.0": crate::runtime::common,
        "umari:sqlite/types@0.1.0": crate::runtime::sqlite,
        "umari:sqlite/connection@0.1.0": crate::runtime::sqlite,
        "umari:sqlite/statement@0.1.0": crate::runtime::sqlite,
    },
});

#[macro_export]
macro_rules! export_policy {
    ($ty:path) => {
        type ExportedPolicy = $crate::runtime::policy::PolicyExport<$ty>;
        $crate::runtime::policy::export!(ExportedPolicy with_types_in $crate::runtime::policy);

        // $crate::runtime::policy::export!({
        //     ty: $crate::runtime::policy::PolicyExport<$ty>,
        //     with_types_in: $crate::runtime::policy,
        // });
    };
}

pub struct PolicyExport<T>(PhantomData<T>);

pub struct PolicyState<T> {
    inner: RefCell<T>,
}

impl<T> Guest for PolicyExport<T>
where
    T: Policy + 'static,
{
    type Policy = PolicyState<T>;
}

impl<T> GuestPolicy for PolicyState<T>
where
    T: Policy + 'static,
{
    fn new() -> Self
    where
        Self: Sized,
    {
        let state = T::init().expect("policy init failed");
        PolicyState {
            inner: RefCell::new(state),
        }
    }

    fn query(&self) -> EventQuery {
        self.inner.borrow().query().into()
    }

    fn handle(&self, stored_event: StoredEvent) -> Vec<CommandSubmission> {
        let Some(event) = common::transform_stored_event::<T::Query>(stored_event) else {
            return vec![];
        };

        self.inner
            .borrow_mut()
            .handle(event)
            .unwrap_or_else(|err| panic!("policy handle error: {err}"))
    }
}
