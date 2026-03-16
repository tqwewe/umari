use axum::http::HeaderMap;
use maud::Markup;

use crate::layout::page;

pub fn is_htmx(headers: &HeaderMap) -> bool {
    headers
        .get("HX-Request")
        .is_some_and(|v| v == "true")
}

pub fn respond(headers: &HeaderMap, title: &str, content: Markup) -> Markup {
    if is_htmx(headers) {
        content
    } else {
        page(title, content)
    }
}
