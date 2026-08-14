use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use maud::{Markup, html};
use serde::Deserialize;
use serde_json::Value;
use tephra::{Event, EventType, Position, Query as EventQuery, QueryItem, Tag, Tags};
use umari_core::event::StoredEventData;
use uuid::Uuid;

use crate::{
    UiState,
    error::HtmlError,
    event_decode::{EventData, decrypt_event_data},
    htmx::respond_wide,
};

#[derive(Deserialize, Default)]
pub struct EventsQuery {
    pub types: Option<String>,
    pub tags: Option<String>,
    pub limit: Option<u32>,
}

struct EventView {
    position: u64,
    uuid: Option<Uuid>,
    event_type: String,
    tags: Vec<String>,
    timestamp: DateTime<Utc>,
    correlation_id: Uuid,
    causation_id: Uuid,
    triggering_event_id: Option<Uuid>,
    encryption_scope: Option<String>,
    data: EventData,
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
        EventQuery::all()
    } else {
        let types = types
            .iter()
            .map(|ty| EventType::new(ty.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| HtmlError::internal(err.to_string()))?;
        let tags = Tags::new(
            tags.iter()
                .map(|tag| Tag::new(tag.clone()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| HtmlError::internal(err.to_string()))?,
        )
        .map_err(|err| HtmlError::internal(err.to_string()))?;
        EventQuery::item(QueryItem::new(types, tags))
    };

    // A backwards read yields newest-first and does work proportional to `limit`, so a page
    // load never scans the whole store. The blocking scan runs off the async runtime.
    let handle = state.event_store.clone();
    let cap = limit as u64;
    let raw_events = tokio::task::spawn_blocking(move || {
        let mut reads = handle.read_back(&query, Position::MAX, Some(cap));
        let mut out: Vec<(Position, Event)> = Vec::new();
        while let Some(item) = reads.next() {
            let seq = item?;
            out.push((seq.position, seq.event.to_owned()));
        }
        Ok::<_, tephra::ReadError>(out)
    })
    .await
    .map_err(|err| HtmlError::internal(err.to_string()))?
    .map_err(|err| HtmlError::internal(err.to_string()))?;

    let mut events: Vec<EventView> = Vec::with_capacity(raw_events.len());
    // `read_back` already yields newest-first.
    for (position, event) in raw_events {
        let stored: StoredEventData<Value> = match serde_json::from_slice(event.data()) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let data =
            decrypt_event_data(&state.module_store_ref, &stored, Some(stored.event_id)).await;

        events.push(EventView {
            position: position.get(),
            uuid: Some(stored.event_id),
            event_type: event.event_type().to_string(),
            tags: event.tags().map(|tag| tag.to_string()).collect(),
            timestamp: stored.timestamp,
            correlation_id: stored.correlation_id,
            causation_id: stored.causation_id,
            triggering_event_id: stored.triggering_event_id,
            encryption_scope: stored.encryption_scope,
            data,
        });
    }

    // Assign color indices per unique correlation_id
    let mut correlation_colors: HashMap<Uuid, usize> = HashMap::new();
    let mut next_color = 0usize;
    for ev in &events {
        correlation_colors
            .entry(ev.correlation_id)
            .or_insert_with(|| {
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
        row_meta.push(RowMeta {
            color_idx,
            show_separator,
        });
        prev_causation = Some(ev.causation_id);
    }

    let types_val = params.types.as_deref().unwrap_or("").to_string();
    let tags_val = params.tags.as_deref().unwrap_or("").to_string();
    let event_count = events.len();

    let content = html! {
        h2 class="text-2xl font-semibold text-gray-900 dark:text-gray-100 mb-6" { "Events" }

        form hx-get="/ui/events" hx-target="#content" hx-push-url="true"
            class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-lg p-4 mb-6" {
            div class="grid grid-cols-3 gap-4" {
                div {
                    label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1" { "Types (comma-separated)" }
                    input type="text" name="types" value=(types_val)
                        placeholder="e.g. UserCreated,OrderPlaced"
                        class="w-full border border-gray-300 dark:border-gray-600 rounded px-3 py-1.5 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-300";
                }
                div {
                    label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1" { "Tags (comma-separated)" }
                    input type="text" name="tags" value=(tags_val)
                        placeholder="e.g. user_id:abc123"
                        class="w-full border border-gray-300 dark:border-gray-600 rounded px-3 py-1.5 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-300";
                }
                div class="flex items-end gap-2" {
                    div class="flex-1" {
                        label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1" { "Limit" }
                        input type="number" name="limit" value=(limit)
                            class="w-full border border-gray-300 dark:border-gray-600 rounded px-3 py-1.5 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-300";
                    }
                    button type="submit"
                        class="px-4 py-1.5 bg-indigo-600 text-white text-sm font-medium rounded hover:bg-indigo-700 transition-colors" {
                        "Search"
                    }
                }
            }
        }

        @if events.is_empty() {
            div class="text-center text-gray-400 dark:text-gray-600 py-16" {
                p class="text-lg" { "No events found" }
                p class="text-sm mt-1" { "Try adjusting the filters or execute a command" }
            }
        } @else {
            div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden" {
                table class="w-full text-sm" {
                    thead {
                        tr class="border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800" {
                            th class="w-4" {}
                            th class="text-left px-3 py-2 text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider w-16" { "Pos" }
                            th class="text-left px-3 py-2 text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider" { "Type" }
                            th class="text-left px-3 py-2 text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider" { "Tags" }
                            th class="text-left px-3 py-2 text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider w-44" { "Timestamp" }
                            th class="text-left px-3 py-2 text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider w-28" { "Correlation" }
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
                            tr onclick=(toggle_js) class="border-b border-gray-100 dark:border-gray-700/50 hover:bg-gray-50 dark:hover:bg-gray-800 cursor-pointer" style=(row_style) {
                                td class="pl-3 w-4 align-middle" {
                                    svg class="chev text-gray-400 dark:text-gray-600" style="transition:transform 0.15s" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" {
                                        polyline points="9 18 15 12 9 6" {}
                                    }
                                }
                                td class="px-3 py-2 text-gray-500 dark:text-gray-500 font-mono text-xs" { (ev.position) }
                                td class="px-3 py-2 font-mono text-xs text-gray-900 dark:text-gray-100" {
                                    (ev.event_type)
                                    @if let Some(scope) = &ev.encryption_scope {
                                        svg class="inline ml-1 text-gray-400 dark:text-gray-500" title=(scope) width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:middle" {
                                            rect x="3" y="11" width="18" height="11" rx="2" ry="2" {}
                                            path d="M7 11V7a5 5 0 0 1 10 0v4" {}
                                        }
                                    }
                                }
                                td class="px-3 py-2" {
                                    div class="flex flex-wrap gap-1" {
                                        @for tag in &ev.tags {
                                            span class="px-1.5 py-0.5 bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300 rounded text-xs" { (tag) }
                                        }
                                    }
                                }
                                td class="px-3 py-2 text-xs text-gray-600 dark:text-gray-400 whitespace-nowrap"
                                    title=(ev.timestamp.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()) {
                                    (ev.timestamp.format("%Y-%m-%d %H:%M:%S UTC"))
                                }
                                td class="px-3 py-2" {
                                    span class="px-1.5 py-0.5 rounded text-xs font-mono"
                                        style=(badge_style)
                                        title=(ev.correlation_id.to_string()) {
                                        (&ev.correlation_id.to_string()[..8])
                                    }
                                }
                            }
                            tr id=(detail_id) style="display:none" {
                                td colspan="6" class="bg-gray-50 dark:bg-gray-900 border-b border-gray-100 dark:border-gray-700/50" style=(format!("border-left: 3px solid {border_color}")) {
                                    div class="flex flex-wrap gap-6 px-4 py-2 border-b border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 text-xs text-gray-500 dark:text-gray-400" {
                                        @if let Some(id) = ev.uuid {
                                            @let full = id.to_string();
                                            span {
                                                "Event ID: "
                                                span class="font-mono text-gray-700 dark:text-gray-300 cursor-pointer hover:text-indigo-600"
                                                    title=(full)
                                                    onclick=(format!("navigator.clipboard.writeText('{full}')")) {
                                                    (&full[..8])
                                                }
                                            }
                                        }
                                        @let corr = ev.correlation_id.to_string();
                                        span {
                                            "Correlation: "
                                            span class="font-mono text-gray-700 dark:text-gray-300 cursor-pointer hover:text-indigo-600"
                                                title=(corr)
                                                onclick=(format!("navigator.clipboard.writeText('{corr}')")) {
                                                (&corr[..8])
                                            }
                                        }
                                        @let caus = ev.causation_id.to_string();
                                        span {
                                            "Causation: "
                                            span class="font-mono text-gray-700 dark:text-gray-300 cursor-pointer hover:text-indigo-600"
                                                title=(caus)
                                                onclick=(format!("navigator.clipboard.writeText('{caus}')")) {
                                                (&caus[..8])
                                            }
                                        }
                                        @if let Some(tid) = ev.triggering_event_id {
                                            @let trig = tid.to_string();
                                            span {
                                                "Triggered by: "
                                                span class="font-mono text-gray-700 dark:text-gray-300 cursor-pointer hover:text-indigo-600"
                                                    title=(trig)
                                                    onclick=(format!("navigator.clipboard.writeText('{trig}')")) {
                                                    (&trig[..8])
                                                }
                                            }
                                        }
                                        @if let Some(scope) = &ev.encryption_scope {
                                            span {
                                                "Encrypted: "
                                                span class="font-mono text-gray-700 dark:text-gray-300" { (scope) }
                                            }
                                        }
                                    }
                                    @match &ev.data {
                                        EventData::Plain(v) | EventData::Decrypted(v) => {
                                            pre class="ev-json text-xs text-gray-800 dark:text-gray-200 whitespace-pre-wrap break-all px-4 py-3" {
                                                (serde_json::to_string_pretty(v).unwrap_or_default())
                                            }
                                        }
                                        EventData::CryptoShredded => {
                                            p class="text-xs text-gray-400 dark:text-gray-500 italic px-4 py-3" {
                                                "🔒 Data unavailable — encryption key has been deleted (crypto-shredded)"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            p class="text-xs text-gray-400 dark:text-gray-600 mt-2" { "showing " (event_count) " events (newest first)" }
            script {
                (maud::PreEscaped(r#"
(function() {
  function highlight(text) {
    return text.replace(
      /("(?:\\u[0-9a-fA-F]{4}|\\[^u]|[^\\"])*"(?:\s*:)?|true|false|null|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?)/g,
      function(m) {
        if (m[0] === '"') {
          return m.slice(-1) === ':'
            ? '<span style="color:#6366f1">' + m + '</span>'
            : '<span style="color:#16a34a">' + m + '</span>';
        }
        if (m === 'true' || m === 'false') return '<span style="color:#d97706">' + m + '</span>';
        if (m === 'null')  return '<span style="color:#9ca3af">' + m + '</span>';
        return '<span style="color:#7c3aed">' + m + '</span>';
      }
    );
  }
  document.querySelectorAll('pre.ev-json').forEach(function(el) {
    el.innerHTML = highlight(el.textContent);
  });
})();
                "#))
            }
        }
    };

    Ok(respond_wide(&headers, "Events", content))
}
