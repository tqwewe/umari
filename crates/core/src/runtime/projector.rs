use std::{cell::RefCell, marker::PhantomData};

pub use self::exports::umari::projector::projector::{Guest, GuestProjector};
use crate::{
    projector::Projector,
    runtime::common::{self, EventQuery, StoredEvent},
};

wit_bindgen::generate!({
    path: "../../wit/projector",
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
macro_rules! export_projector {
    ($ty:path) => {
        type ExportedProjection = $crate::runtime::projector::ProjectorExport<$ty>;
        $crate::runtime::projector::export!(ExportedProjection with_types_in $crate::runtime::projector);

        // $crate::runtime::projector::export!({
        //     ty: $crate::runtime::projector::ProjectorExport<$ty>,
        //     with_types_in: $crate::runtime::projector,
        // });
    };
}

pub struct ProjectorExport<T>(PhantomData<T>);

pub struct ProjectorState<T> {
    inner: RefCell<T>,
}

impl<T: Projector + 'static> Guest for ProjectorExport<T> {
    type Projector = ProjectorState<T>;
}

impl<T: Projector + 'static> GuestProjector for ProjectorState<T> {
    fn new() -> Self
    where
        Self: Sized,
    {
        let state = T::init().expect("projector init failed");
        ProjectorState {
            inner: RefCell::new(state),
        }
    }

    fn query(&self) -> EventQuery {
        self.inner.borrow().query().into()
    }

    fn handle(&self, stored_event: StoredEvent) {
        let Some(event) = common::transform_stored_event::<T::Query>(stored_event) else {
            return;
        };

        self.inner
            .borrow_mut()
            .handle(event)
            .unwrap_or_else(|err| panic!("projector handle error: {err}"))
    }
}
