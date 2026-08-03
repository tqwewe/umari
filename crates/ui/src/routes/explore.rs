use axum::{Form, extract::State, http::HeaderMap};
use maud::{Markup, html};
use serde::Deserialize;

use crate::{
    UiState,
    components::{projection_editor, projection_error, projection_result},
    htmx::respond_wide,
    projection::run_projection,
};

const STARTER_SCRIPT: &str = r#"// In-memory projection — nothing is uploaded or saved.
// Declare the events to fold, build up state, then optionally select rows.
project({
  // Which events to fold. A bare type, or { type, scope: { field: "value" } }.
  events: ["example.event"],

  // Initial state (kept compact — this streams over the whole history).
  init: () => ({ count: 0 }),

  // Called once per event, oldest first. `event` has: type, data,
  // position, timestamp, tags, correlationId, ...
  handle: (event, state) => {
    state.count++;
  },

  // Optional: turn state into rows for a table. Omit to see the raw state.
  select: (state) => [{ metric: "events", value: state.count }],
});
"#;

pub async fn explore_page(headers: HeaderMap) -> Markup {
    let content = html! {
        h2 class="text-2xl font-semibold text-gray-900 dark:text-gray-100 mb-2" { "Explore" }
        p class="text-sm text-gray-500 dark:text-gray-400 mb-6" {
            "Run an ad-hoc, in-memory projection over the event stream. Nothing is uploaded or persisted."
        }
        (projection_editor("/ui/explore/run", STARTER_SCRIPT))
    };
    respond_wide(&headers, "Explore", content)
}

#[derive(Deserialize)]
pub struct ProjectionForm {
    pub script: String,
    #[serde(default)]
    pub limit: Option<String>,
}

pub async fn run_projection_handler(
    State(state): State<UiState>,
    Form(form): Form<ProjectionForm>,
) -> Markup {
    let limit = form.limit.as_deref().and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.parse::<u32>().ok()).flatten()
    });

    match run_projection(
        &state.event_store,
        &state.module_store_ref,
        form.script,
        limit,
    )
    .await
    {
        Ok(outcome) => projection_result(&outcome.result, &outcome.logs),
        Err(err) => projection_error(&err.message()),
    }
}
