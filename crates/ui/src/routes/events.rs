use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use maud::{Markup, html};
use serde::Deserialize;
use serde_json::Value;
use umadb_dcb::{DcbEventStoreAsync, DcbQuery, DcbQueryItem};
use umari_core::event::StoredEventData;
use uuid::Uuid;

use crate::{UiState, error::HtmlError, htmx::respond_wide};

#[derive(Deserialize, Default)]
pub struct EventsQuery {
    pub types: Option<String>,
    pub tags: Option<String>,
    pub limit: Option<u32>,
}

struct EventView {
    position: u64,
    event_type: String,
    tags: Vec<String>,
    timestamp: DateTime<Utc>,
    correlation_id: Uuid,
    causation_id: Uuid,
    data: Value,
}

// Border-left colors for correlation groups (inline styles to avoid Tailwind purging)
const CORRELATION_BORDER_COLORS: &[&str] = &[
    "#6366f1", // indigo-500
    "#10b981", // emerald-500
    "#f59e0b", // amber-500
    "#f43f5e", // rose-500
    "#8b5cf6", // violet-500
    "#06b6d4", // cyan-500
    "#f97316", // orange-500
    "#ec4899", // pink-500
];

const CORRELATION_BADGE_COLORS: &[(&str, &str)] = &[
    ("#e0e7ff", "#3730a3"), // indigo
    ("#d1fae5", "#065f46"), // emerald
    ("#fef3c7", "#92400e"), // amber
    ("#ffe4e6", "#9f1239"), // rose
    ("#ede9fe", "#4c1d95"), // violet
    ("#cffafe", "#164e63"), // cyan
    ("#ffedd5", "#7c2d12"), // orange
    ("#fce7f3", "#831843"), // pink
];

pub async fn list_events(
    State(state): State<UiState>,
    Query(params): Query<EventsQuery>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let limit = params.limit.unwrap_or(200);

    let types: Vec<String> = params
        .types
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let tags: Vec<String> = params
        .tags
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let query = if types.is_empty() && tags.is_empty() {
        None
    } else {
        let mut item = DcbQueryItem::new();
        if !types.is_empty() {
            item = item.types(types.iter().map(String::as_str));
        }
        if !tags.is_empty() {
            item = item.tags(tags.iter().map(String::as_str));
        }
        Some(DcbQuery::new().item(item))
    };

    let (raw_events, _head) = state
        .event_store
        .read(query, None, true, Some(limit), false)
        .await
        .map_err(|err| HtmlError::internal(err.to_string()))?
        .collect_with_head()
        .await
        .map_err(|err| HtmlError::internal(err.to_string()))?;

    let mut events: Vec<EventView> = Vec::with_capacity(raw_events.len());
    for seq in raw_events {
        let stored: StoredEventData<Value> = match serde_json::from_slice(&seq.event.data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        events.push(EventView {
            position: seq.position,
            event_type: seq.event.event_type,
            tags: seq.event.tags,
            timestamp: stored.timestamp,
            correlation_id: stored.correlation_id,
            causation_id: stored.causation_id,
            data: stored.data,
        });
    }

    // Assign color indices per unique correlation_id
    let mut correlation_colors: HashMap<Uuid, usize> = HashMap::new();
    let mut next_color = 0usize;
    for ev in &events {
        correlation_colors.entry(ev.correlation_id).or_insert_with(|| {
            let idx = next_color % CORRELATION_BORDER_COLORS.len();
            next_color += 1;
            idx
        });
    }

    // Pre-compute per-row metadata
    struct RowMeta {
        color_idx: usize,
        show_separator: bool,
    }
    let mut row_meta: Vec<RowMeta> = Vec::with_capacity(events.len());
    let mut prev_causation: Option<Uuid> = None;
    for ev in &events {
        let color_idx = correlation_colors[&ev.correlation_id];
        let show_separator = prev_causation.is_some_and(|p| p != ev.causation_id);
        row_meta.push(RowMeta { color_idx, show_separator });
        prev_causation = Some(ev.causation_id);
    }

    let types_val = params.types.as_deref().unwrap_or("").to_string();
    let tags_val = params.tags.as_deref().unwrap_or("").to_string();
    let event_count = events.len();

    let content = html! {
        h2 class="text-2xl font-semibold text-gray-900 mb-6" { "Events" }

        form hx-get="/ui/events" hx-target="#content" hx-push-url="true"
            class="bg-white border border-gray-200 rounded-lg p-4 mb-6" {
            div class="grid grid-cols-3 gap-4" {
                div {
                    label class="block text-xs font-medium text-gray-500 mb-1" { "Types (comma-separated)" }
                    input type="text" name="types" value=(types_val)
                        placeholder="e.g. UserCreated,OrderPlaced"
                        class="w-full border border-gray-300 rounded px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-300";
                }
                div {
                    label class="block text-xs font-medium text-gray-500 mb-1" { "Tags (comma-separated)" }
                    input type="text" name="tags" value=(tags_val)
                        placeholder="e.g. user_id:abc123"
                        class="w-full border border-gray-300 rounded px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-300";
                }
                div class="flex items-end gap-2" {
                    div class="flex-1" {
                        label class="block text-xs font-medium text-gray-500 mb-1" { "Limit" }
                        input type="number" name="limit" value=(limit)
                            class="w-full border border-gray-300 rounded px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-300";
                    }
                    button type="submit"
                        class="px-4 py-1.5 bg-indigo-600 text-white text-sm font-medium rounded hover:bg-indigo-700 transition-colors" {
                        "Search"
                    }
                }
            }
        }

        @if events.is_empty() {
            div class="text-center text-gray-400 py-16" {
                p class="text-lg" { "No events found" }
                p class="text-sm mt-1" { "Try adjusting the filters or execute a command" }
            }
        } @else {
            div class="bg-white border border-gray-200 rounded-lg overflow-hidden" {
                table class="w-full text-sm" {
                    thead {
                        tr class="border-b border-gray-200 bg-gray-50" {
                            th class="w-4" {}
                            th class="text-left px-3 py-2 text-xs font-semibold text-gray-500 uppercase tracking-wider w-16" { "Pos" }
                            th class="text-left px-3 py-2 text-xs font-semibold text-gray-500 uppercase tracking-wider" { "Type" }
                            th class="text-left px-3 py-2 text-xs font-semibold text-gray-500 uppercase tracking-wider" { "Tags" }
                            th class="text-left px-3 py-2 text-xs font-semibold text-gray-500 uppercase tracking-wider w-44" { "Timestamp" }
                            th class="text-left px-3 py-2 text-xs font-semibold text-gray-500 uppercase tracking-wider w-28" { "Correlation" }
                            th class="text-left px-3 py-2 text-xs font-semibold text-gray-500 uppercase tracking-wider w-28" { "Causation" }
                        }
                    }
                    tbody {
                        @for (ev, meta) in events.iter().zip(row_meta.iter()) {
                            @let border_color = CORRELATION_BORDER_COLORS[meta.color_idx];
                            @let (badge_bg, badge_text) = CORRELATION_BADGE_COLORS[meta.color_idx];
                            @let row_style = format!("border-left: 3px solid {border_color}");
                            @let badge_style = format!("background:{badge_bg};color:{badge_text}");
                            @let detail_id = format!("ev-data-{}", ev.position);
                            @let toggle_js = format!("var r=document.getElementById('{detail_id}'),open=r.style.display==='table-row';r.style.display=open?'none':'table-row';this.querySelector('.chev').style.transform=open?'':'rotate(90deg)'");
                            @if meta.show_separator {
                                tr style="border-top: 1px dashed #e5e7eb" {}
                            }
                            tr onclick=(toggle_js) class="border-b border-gray-100 hover:bg-gray-50 cursor-pointer" style=(row_style) {
                                td class="pl-3 w-4 align-middle" {
                                    svg class="chev text-gray-400" style="transition:transform 0.15s" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" {
                                        polyline points="9 18 15 12 9 6" {}
                                    }
                                }
                                td class="px-3 py-2 text-gray-500 font-mono text-xs" { (ev.position) }
                                td class="px-3 py-2 font-mono text-xs text-gray-900" { (ev.event_type) }
                                td class="px-3 py-2" {
                                    div class="flex flex-wrap gap-1" {
                                        @for tag in &ev.tags {
                                            span class="px-1.5 py-0.5 bg-gray-100 text-gray-600 rounded text-xs" { (tag) }
                                        }
                                    }
                                }
                                td class="px-3 py-2 text-xs text-gray-600 whitespace-nowrap" {
                                    (ev.timestamp.format("%Y-%m-%d %H:%M:%S UTC"))
                                }
                                td class="px-3 py-2" {
                                    span class="px-1.5 py-0.5 rounded text-xs font-mono"
                                        style=(badge_style)
                                        title=(ev.correlation_id.to_string()) {
                                        (&ev.correlation_id.to_string()[..8])
                                    }
                                }
                                td class="px-3 py-2" {
                                    span class="px-1.5 py-0.5 bg-gray-100 text-gray-700 rounded text-xs font-mono"
                                        title=(ev.causation_id.to_string()) {
                                        (&ev.causation_id.to_string()[..8])
                                    }
                                }
                            }
                            tr id=(detail_id) style="display:none" {
                                td colspan="7" class="px-4 py-3 bg-gray-50 border-b border-gray-100" style=(format!("border-left: 3px solid {border_color}")) {
                                    pre class="text-xs text-gray-800 whitespace-pre-wrap break-all" {
                                        (serde_json::to_string_pretty(&ev.data).unwrap_or_default())
                                    }
                                }
                            }
                        }
                    }
                }
            }
            p class="text-xs text-gray-400 mt-2" { "showing " (event_count) " events (newest first)" }
        }
    };

    Ok(respond_wide(&headers, "Events", content))
}
