use wasmtime::component::bindgen;

pub use self::umari::command::{types::*, *};

use crate::wit::{self, BasicComponentState};

bindgen!({
    path: "../../wit/command",
    world: "command",
    imports: { default: tracing | trappable },
    exports: { default: async },
    with: {
        "umari:common": crate::wit::common,
    }
});

impl Host for BasicComponentState {}

impl Host for wit::EventHandlerComponentState {}
