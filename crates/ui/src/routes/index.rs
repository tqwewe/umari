use axum::{extract::State, http::HeaderMap};
use maud::Markup;

use crate::{UiState, error::HtmlError, htmx::respond};

use super::commands::commands_list_fragment;

pub async fn index(
    State(state): State<UiState>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let content = commands_list_fragment(&state).await?;
    Ok(respond(&headers, "Commands", content))
}
