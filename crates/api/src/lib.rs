pub mod error;
mod routes;

use axum::{Router, routing::post};
use kameo::actor::ActorRef;
use tokio::{io, net::ToSocketAddrs};
use umari_runtime::{command::actor::CommandActor, module_store::actor::ModuleStoreActor};

use crate::routes::execute::execute;

#[derive(Clone, Debug)]
pub struct AppState {
    pub module_store_ref: ActorRef<ModuleStoreActor>,
    pub command_ref: ActorRef<CommandActor>,
}

pub async fn start_server(addr: impl ToSocketAddrs, state: AppState) -> io::Result<()> {
    let app = Router::new()
        .route("/execute/{name}", post(execute))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}
