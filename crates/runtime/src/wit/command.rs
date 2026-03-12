use wasmtime::component::bindgen;

pub use self::umari::command::types::*;

use crate::wit::BasicComponentState;

bindgen!({
    path: "../../wit/command",
    world: "command",
    exports: { default: async },
    with: {
        "umari:common/types@0.1.0": crate::wit::common,
    }
});

impl Host for BasicComponentState {}
