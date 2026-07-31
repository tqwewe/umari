use axum::{extract::State, http::header, response::IntoResponse};

use crate::AppState;

pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        state.metrics_handle.render(),
    )
}
