pub mod error;
mod routes;

use axum::{Router, routing::post};
use kameo::actor::ActorRef;
use rivo_runtime::{command::actor::CommandActor, store::actor::StoreActor};
use tokio::{io, net::ToSocketAddrs};

use crate::routes::execute::execute;

#[derive(Clone, Debug)]
pub struct AppState {
    pub store_ref: ActorRef<StoreActor>,
    pub command_ref: ActorRef<CommandActor>,
}

pub async fn start_server(addr: impl ToSocketAddrs, state: AppState) -> io::Result<()> {
    let app = Router::new()
        .route("/execute/{name}", post(execute))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}
