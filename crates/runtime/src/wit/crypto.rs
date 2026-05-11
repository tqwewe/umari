use wasmtime::component::bindgen;

use crate::{module_store::actor::DeleteCryptoKey, wit::EventHandlerComponentState};

pub use self::umari::crypto::{keys::*, *};

bindgen!({
    path: "../umari/wit/crypto",
    world: "crypto",
    imports: { default: async | tracing | trappable },
    exports: { default: async },
});

impl Host for EventHandlerComponentState {
    async fn delete_key(&mut self, scope: String) -> wasmtime::Result<()> {
        self.module_store_ref
            .ask(DeleteCryptoKey {
                scope: scope.into(),
            })
            .await?;
        Ok(())
    }
}
