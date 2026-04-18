use std::{cell::RefCell, fmt, marker::PhantomData};

pub use self::exports::umari::effect::effect::{Guest, GuestEffect};
use crate::{
    effect::Effect,
    runtime::common::{self, EventQuery, StoredEvent},
};

wit_bindgen::generate!({
    path: "../../wit/effect",
    pub_export_macro: true,
    with: {
        "umari:command/executor@0.1.0": crate::runtime::command,
        "umari:command/types@0.1.0": crate::runtime::command,
        "umari:common/types@0.1.0": crate::runtime::common,
        "umari:sqlite/types@0.1.0": crate::runtime::sqlite,
        "umari:sqlite/connection@0.1.0": crate::runtime::sqlite,
        "umari:sqlite/statement@0.1.0": crate::runtime::sqlite,
        "wasi:clocks/monotonic-clock@0.2.8": wasip2::clocks::monotonic_clock,
        "wasi:io/error@0.2.8": wasip2::io,
        "wasi:io/poll@0.2.8": wasip2::io::poll,
        "wasi:io/streams@0.2.8": wasip2::io::streams,
        "wasi:http/types@0.2.8": wasip2::http::types,
        "wasi:http/outgoing-handler@0.2.8": wasip2::http::outgoing_handler,
    },
});

#[macro_export]
macro_rules! export_effect {
    ($ty:path) => {
        type ExportedEffect = $crate::runtime::effect::EffectExport<$ty>;
        $crate::runtime::effect::export!(ExportedEffect with_types_in $crate::runtime::effect);

        // $crate::runtime::effect::export!({
        //     ty: $crate::runtime::effect::EffectExport<$ty>,
        //     with_types_in: $crate::runtime::effect,
        // });
    };
}

pub struct EffectExport<T>(PhantomData<T>);

pub struct EffectState<T> {
    inner: RefCell<T>,
}

impl<T> Guest for EffectExport<T>
where
    T: Effect + 'static,
    T::Error: fmt::Display,
{
    type Effect = EffectState<T>;
}

impl<T> GuestEffect for EffectState<T>
where
    T: Effect + 'static,
    T::Error: fmt::Display,
{
    fn new() -> Self
    where
        Self: Sized,
    {
        let state = T::init().expect("effect init failed");
        EffectState {
            inner: RefCell::new(state),
        }
    }

    fn query(&self) -> EventQuery {
        self.inner.borrow().query().into()
    }

    fn partition_key(&self, stored_event: StoredEvent) -> Option<String> {
        let event = common::transform_stored_event::<T::Query>(stored_event)?;
        self.inner.borrow().partition_key(event)
    }

    fn handle(&self, stored_event: StoredEvent) {
        let Some(event) = common::transform_stored_event::<T::Query>(stored_event) else {
            return;
        };

        self.inner
            .borrow_mut()
            .handle(event)
            .unwrap_or_else(|err| panic!("effect handle error: {err}"))
    }
}
