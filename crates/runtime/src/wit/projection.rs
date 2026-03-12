use wasmtime::component::bindgen;

pub use self::exports::umari::projection::projection_runner::Error;

bindgen!({
    path: "../../wit/projection",
    world: "projection",
    exports: { default: async },
    with: {
        "umari:common/types@0.1.0": crate::wit::common,
        "umari:sqlite/types@0.1.0": crate::wit::sqlite,
        "umari:sqlite/connection@0.1.0": crate::wit::sqlite,
        "umari:sqlite/statement@0.1.0": crate::wit::sqlite,
    }
});
