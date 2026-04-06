use axum::http::HeaderMap;
use maud::Markup;

use crate::layout::{page, wide_page, width_wrapper};

pub fn is_htmx(headers: &HeaderMap) -> bool {
    headers
        .get("HX-Request")
        .is_some_and(|v| v == "true")
}

pub fn respond(headers: &HeaderMap, title: &str, content: Markup) -> Markup {
    if is_htmx(headers) {
        width_wrapper(content, false)
    } else {
        page(title, content)
    }
}

pub fn respond_wide(headers: &HeaderMap, title: &str, content: Markup) -> Markup {
    if is_htmx(headers) {
        width_wrapper(content, true)
    } else {
        wide_page(title, content)
    }
}
