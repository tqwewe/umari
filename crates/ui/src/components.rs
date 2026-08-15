use std::{collections::HashMap, path::PathBuf, sync::Arc};

use maud::{Markup, PreEscaped, html};
use rusqlite::{Connection, OpenFlags};
use schemars::Schema;
use semver::Version;
use serde_json::Value;
use umari_runtime::{
    module_store::{Module, ModuleType, ModuleVersionInfo},
    output::{LogEntry, LogStream},
};

#[derive(Debug)]
pub struct ModuleHealth {
    pub healthy: bool,
    pub shutdown_reason: Option<String>,
    pub last_position: Option<u64>,
}

pub fn module_summary_table(
    module_type: ModuleType,
    names: &[String],
    active_modules: &[Module],
    health: &HashMap<Arc<str>, ModuleHealth>,
) -> Markup {
    let type_path = match module_type {
        ModuleType::Command => "commands",
        ModuleType::Projector => "projectors",
        ModuleType::Effect => "effects",
    };

    let show_position = module_type != ModuleType::Command;

    html! {
        div class="overflow-hidden rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900" {
            table class="w-full text-sm" {
                thead {
                    tr class="bg-gray-50 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700" {
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider" { "Name" }
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider" { "Active Version" }
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider" { "Status" }
                        @if show_position {
                            th class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider" { "Position" }
                        }
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider" { "SHA256" }
                    }
                }
                tbody {
                    @if names.is_empty() {
                        tr {
                            td colspan=(if show_position { "5" } else { "4" }) class="px-4 py-3 text-sm text-gray-500 dark:text-gray-400" { "No modules uploaded yet." }
                        }
                    }
                    @for name in names {
                        @let active = active_modules.iter().find(|m| m.name == *name);
                        @let module_health = health.get(name.as_str());
                        tr class="border-b border-gray-100 dark:border-gray-700/50 last:border-0 hover:bg-gray-50 dark:hover:bg-gray-800" {
                            td class="px-4 py-3" {
                                a href={"/ui/" (type_path) "/" (name)}
                                    hx-get={"/ui/" (type_path) "/" (name)}
                                    hx-target="#content"
                                    hx-push-url={"/ui/" (type_path) "/" (name)}
                                    class="text-indigo-600 dark:text-indigo-400 hover:text-indigo-800 dark:hover:text-indigo-300 font-medium"
                                    { (name) }
                            }
                            td class="px-4 py-3 text-gray-700 dark:text-gray-300" {
                                @if let Some(a) = active {
                                    span class="text-emerald-600 font-medium" { (a.version) }
                                } @else {
                                    span class="text-gray-400 dark:text-gray-500" { "—" }
                                }
                            }
                            td class="px-4 py-3" {
                                @if active.is_none() {
                                    // no active version — show nothing
                                } @else if let Some(h) = module_health {
                                    @if h.healthy {
                                        span class="text-emerald-500" { "● Running" }
                                    } @else {
                                        @let title = h.shutdown_reason.as_deref().unwrap_or("");
                                        span class="text-red-500" title=(title) { "● Stopped" }
                                    }
                                } @else {
                                    span class="text-amber-500" { "● Not running" }
                                }
                            }
                            @if show_position {
                                td class="px-4 py-3 text-gray-500 dark:text-gray-400 font-mono text-xs" {
                                    @if let Some(pos) = module_health.and_then(|h| h.last_position) {
                                        (pos)
                                    }
                                }
                            }
                            td class="px-4 py-3 text-gray-500 dark:text-gray-400 font-mono text-xs" {
                                @if let Some(a) = active {
                                    @let sha_short = &a.sha256[..12.min(a.sha256.len())];
                                    span title=(a.sha256) { (sha_short) "…" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn module_status_card(
    module_type: ModuleType,
    name: &str,
    active_version: Option<&Version>,
    health: Option<&ModuleHealth>,
) -> Markup {
    let type_path = match module_type {
        ModuleType::Command => "commands",
        ModuleType::Projector => "projectors",
        ModuleType::Effect => "effects",
    };
    let replay_url = format!("/ui/{type_path}/{name}/replay");

    html! {
        div class="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4 flex items-start justify-between gap-4" {
            div class="flex items-center gap-8" {
                div {
                    p class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1" { "Status" }
                    @if active_version.is_none() {
                        span class="text-gray-400 dark:text-gray-500 text-sm" { "Inactive" }
                    } @else if let Some(h) = health {
                        @if h.healthy {
                            span class="text-emerald-600 text-sm font-medium" { "● Running" }
                        } @else {
                            @let title = h.shutdown_reason.as_deref().unwrap_or("");
                            span class="text-red-500 text-sm font-medium" title=(title) { "● Stopped" }
                        }
                    } @else {
                        span class="text-amber-500 text-sm font-medium" { "● Not running" }
                    }
                }
                div {
                    p class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1" { "Active Version" }
                    @if let Some(v) = active_version {
                        span class="text-emerald-600 font-mono text-sm font-medium" { (v) }
                    } @else {
                        span class="text-gray-400 dark:text-gray-500 text-sm" { "—" }
                    }
                }
                div {
                    p class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1" { "Position" }
                    @if let Some(pos) = health.and_then(|h| h.last_position) {
                        span class="text-gray-700 dark:text-gray-300 font-mono text-sm" { (pos) }
                    } @else {
                        span class="text-gray-400 dark:text-gray-500 text-sm" { "—" }
                    }
                }
            }
            @if active_version.is_some() {
                div class="flex flex-col items-end gap-1 shrink-0" {
                    dialog
                        id="confirm-reset-replay"
                        onclick="if(event.target===this)this.close()"
                        class="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 shadow-xl backdrop:bg-black/40 p-0 w-full max-w-md"
                    {
                        div class="p-6" {
                            h3 class="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2" { "Reset & Replay" }
                            p class="text-sm text-gray-600 dark:text-gray-400 mb-4" {
                                "This will reset the module database and replay all events from position 0. "
                                "Any state built up by this module will be lost."
                            }
                            div class="flex justify-end gap-2" {
                                button
                                    type="button"
                                    onclick="document.getElementById('confirm-reset-replay').close()"
                                    class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                                    { "Cancel" }
                                button
                                    type="button"
                                    hx-post=(replay_url)
                                    hx-target="#replay-status"
                                    hx-swap="innerHTML"
                                    onclick="document.getElementById('confirm-reset-replay').close()"
                                    class="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium rounded-md bg-amber-600 text-white hover:bg-amber-700 transition-colors"
                                    { "↺ Reset & Replay" }
                            }
                        }
                    }
                    button
                        type="button"
                        onclick="document.getElementById('confirm-reset-replay').showModal()"
                        class="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium rounded-md bg-amber-600 text-white hover:bg-amber-700 transition-colors"
                        { "↺ Reset & Replay" }
                    div id="replay-status" class="text-xs text-amber-700" {}
                }
            }
        }
    }
}

pub fn versions_table(
    module_type: ModuleType,
    name: &str,
    mut versions: Vec<ModuleVersionInfo>,
    active_version: Option<&Version>,
) -> Markup {
    versions.sort_unstable_by(|a, b| b.version.cmp(&a.version));
    let module_type_str = match module_type {
        ModuleType::Command => "commands",
        ModuleType::Projector => "projectors",
        ModuleType::Effect => "effects",
    };
    let table_id = format!("versions-table-{name}");

    html! {
        // Confirmation modals for major version changes
        @for info in &versions {
            @let is_active = active_version.is_some_and(|v| v == &info.version);
            @if !is_active && active_version.is_some_and(|av| av.major != info.version.major) {
                @let modal_id = format!("confirm-activate-{name}-{}", info.version);
                @let active_ver_str = active_version.map(|v| v.to_string()).unwrap_or_default();
                dialog
                    id=(modal_id)
                    onclick="if(event.target===this)this.close()"
                    class="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 shadow-xl backdrop:bg-black/40 p-0 w-full max-w-md"
                {
                    div class="p-6" {
                        h3 class="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2" { "Major Version Change" }
                        p class="text-sm text-gray-600 dark:text-gray-400 mb-4" {
                            "Activating version " strong { (info.version) } " will reset the module database, "
                            "as it has a different major version to the currently active version " strong { (active_ver_str) } "."
                        }
                        div class="flex justify-end gap-2" {
                            button
                                type="button"
                                onclick={"document.getElementById('" (modal_id) "').close()"}
                                class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                                { "Cancel" }
                            button
                                type="button"
                                hx-put={"/ui/" (module_type_str) "/" (name) "/active"}
                                hx-vals={"{\"version\":\"" (info.version) "\"}"}
                                hx-target={"#" (table_id)}
                                hx-swap="outerHTML"
                                onclick={"document.getElementById('" (modal_id) "').close()"}
                                class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-red-600 text-white hover:bg-red-700 transition-colors"
                                { "Confirm Activation" }
                        }
                    }
                }
            }
        }
        div class="overflow-hidden rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900" {
            table id=(table_id) class="w-full text-sm" {
                thead {
                    tr class="bg-gray-50 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700" {
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider" { "Version" }
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider" { "Active" }
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider" { "SHA256" }
                        th class="px-4 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider" { "Actions" }
                    }
                }
                tbody {
                    @if versions.is_empty() {
                        tr {
                            td colspan="4" class="px-4 py-3 text-sm text-gray-500 dark:text-gray-400" { "No versions uploaded yet." }
                        }
                    }
                    @for info in &versions {
                        @let is_active = active_version.is_some_and(|v| v == &info.version);
                        @let major_differs = !is_active && active_version.is_some_and(|av| av.major != info.version.major);
                        @let sha_short = &info.sha256[..12.min(info.sha256.len())];
                        tr class="border-b border-gray-100 dark:border-gray-700/50 last:border-0 hover:bg-gray-50 dark:hover:bg-gray-800" {
                            td class="px-4 py-3 text-gray-700 dark:text-gray-300 font-mono text-xs" { (info.version) }
                            td class="px-4 py-3" {
                                @if is_active {
                                    span class="text-emerald-500 font-semibold" { "✓" }
                                }
                            }
                            td class="px-4 py-3 text-gray-500 dark:text-gray-400 font-mono text-xs" {
                                span class="inline-flex items-center gap-1.5" {
                                    span title=(info.sha256) { (sha_short) "…" }
                                    button
                                        type="button"
                                        title="Copy full SHA256"
                                        onclick={"navigator.clipboard.writeText('" (info.sha256) "').then(() => { const el = this; el.textContent = '✓'; setTimeout(() => el.textContent = '⧉', 1500); })"}
                                        class="text-gray-400 dark:text-gray-500 hover:text-gray-600 dark:hover:text-gray-400 transition-colors"
                                        { "⧉" }
                                }
                            }
                            td class="px-4 py-3 text-right" {
                                div class="flex items-center justify-end gap-2" {
                                    @if is_active {
                                        button
                                            hx-delete={"/ui/" (module_type_str) "/" (name) "/active"}
                                            hx-target={"#" (table_id)}
                                            hx-swap="outerHTML"
                                            class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                                            { "Deactivate" }
                                    } @else if major_differs {
                                        @let modal_id = format!("confirm-activate-{name}-{}", info.version);
                                        button
                                            type="button"
                                            onclick={"document.getElementById('" (modal_id) "').showModal()"}
                                            class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
                                            { "Activate" }
                                    } @else {
                                        button
                                            hx-put={"/ui/" (module_type_str) "/" (name) "/active"}
                                            hx-vals={"{\"version\":\"" (info.version) "\"}"}
                                            hx-target={"#" (table_id)}
                                            hx-swap="outerHTML"
                                            class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
                                            { "Activate" }
                                    }
                                    @if !is_active {
                                        button
                                            hx-delete={"/ui/" (module_type_str) "/" (name) "/versions/" (info.version)}
                                            hx-target={"#" (table_id)}
                                            hx-swap="outerHTML"
                                            hx-confirm={"Delete version " (info.version) "?"}
                                            class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors"
                                            { "Delete" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn delete_module_button(module_type: ModuleType, name: &str) -> Markup {
    let module_type_str = match module_type {
        ModuleType::Command => "commands",
        ModuleType::Projector => "projectors",
        ModuleType::Effect => "effects",
    };
    let builds_state = module_type != ModuleType::Command;

    html! {
        dialog
            id="confirm-delete-module"
            onclick="if(event.target===this)this.close()"
            class="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 shadow-xl backdrop:bg-black/40 p-0 w-full max-w-md"
        {
            div class="p-6" {
                h3 class="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2" { "Delete Module" }
                p class="text-sm text-gray-600 dark:text-gray-400 mb-4" {
                    "This permanently deletes " strong { (name) } " and all of its versions"
                    @if builds_state { " along with any state it has built up" }
                    ". This cannot be undone."
                }
                div class="flex justify-end gap-2" {
                    button
                        type="button"
                        onclick="document.getElementById('confirm-delete-module').close()"
                        class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                        { "Cancel" }
                    button
                        type="button"
                        hx-delete={"/ui/" (module_type_str) "/" (name)}
                        hx-target="#delete-module-status"
                        hx-swap="innerHTML"
                        onclick="document.getElementById('confirm-delete-module').close()"
                        class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-red-600 text-white hover:bg-red-700 transition-colors"
                        { "Delete Module" }
                }
            }
        }
        button
            type="button"
            onclick="document.getElementById('confirm-delete-module').showModal()"
            class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md text-red-600 dark:text-red-400 border border-red-200 dark:border-red-900/50 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors"
            { "Delete module" }
        div id="delete-module-status" class="text-xs text-red-700" {}
    }
}

pub fn tabs(id: &str, panels: Vec<(&str, Markup)>) -> Markup {
    let slugs: Vec<String> = panels
        .iter()
        .map(|(l, _)| l.to_lowercase().replace(' ', "-"))
        .collect();
    let labels: Vec<&str> = panels.iter().map(|(l, _)| *l).collect();
    let contents: Vec<Markup> = panels.into_iter().map(|(_, c)| c).collect();
    // Build the init script: restore active tab from location.hash
    let init_js = format!(
        r#"(function(){{
            const hash = location.hash.slice(1);
            const group = document.getElementById('{}');
            if (!hash || !group) return;
            const btns = group.querySelectorAll('[data-tab-slug]');
            btns.forEach((b, i) => {{ if (b.dataset.tabSlug === hash) umariTabs('{}', i, hash); }});
        }})();"#,
        id, id
    );
    html! {
        div id=(id) {
            div class="flex border-b border-gray-200 dark:border-gray-700 mb-6" {
                @for (i, label) in labels.iter().enumerate() {
                    button
                        type="button"
                        data-tab-btn=""
                        data-tab-slug=(slugs[i])
                        onclick=(format!("umariTabs('{}',{},'{}')", id, i, slugs[i]))
                        data-tab-active[i == 0]
                        class="px-4 py-2 text-sm font-medium -mb-px border-b-2 border-transparent text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 hover:border-gray-300 dark:hover:border-gray-600"
                    { (label) }
                }
            }
            @for (i, content) in contents.iter().enumerate() {
                div
                    data-tab-panel=""
                    class=(if i == 0 { "" } else { "hidden" })
                {
                    (content)
                }
            }
        }
        script { (maud::PreEscaped(r#"
            function umariTabs(id, idx, slug) {
                const group = document.getElementById(id);
                group.querySelectorAll('[data-tab-panel]').forEach((p, i) => p.classList.toggle('hidden', i !== idx));
                group.querySelectorAll('[data-tab-btn]').forEach((b, i) => {
                    b.toggleAttribute('data-tab-active', i === idx);
                    b.classList.toggle('border-transparent', i !== idx);
                    b.classList.toggle('text-gray-500', i !== idx);
                });
                history.replaceState(null, '', '#' + slug);
            }
        "#)) (maud::PreEscaped(init_js)) }
    }
}

pub fn output(entries: &[LogEntry]) -> Markup {
    html! {
        section {
            @if entries.is_empty() {
                p class="text-sm text-gray-400 italic" { "no output" }
            } @else {
                div class="overflow-hidden rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900" {
                    table class="w-full text-xs font-mono" {
                        tbody {
                            @for entry in entries {
                                @if matches!(entry.stream, LogStream::System) {
                                    tr class="border-b border-gray-100 dark:border-gray-700/50 last:border-0 bg-gray-50 dark:bg-gray-800" {
                                        td colspan="3" class="px-3 py-1.5 text-center text-xs text-gray-400 dark:text-gray-500 italic" {
                                            (entry.timestamp.format("%H:%M:%S%.3f").to_string())
                                            " — "
                                            (entry.message)
                                        }
                                    }
                                } @else {
                                    @let ts = entry.timestamp.format("%H:%M:%S%.3f").to_string();
                                    @let is_stderr = matches!(entry.stream, LogStream::Stderr);
                                    tr class="border-b border-gray-100 dark:border-gray-700/50 last:border-0" {
                                        td class="px-3 py-1 text-gray-400 dark:text-gray-500 whitespace-nowrap w-28" { (ts) }
                                        td class="px-2 py-1 whitespace-nowrap w-16" {
                                            @if is_stderr {
                                                span class="text-red-500 font-semibold" { "stderr" }
                                            } @else {
                                                span class="text-gray-400 dark:text-gray-500" { "stdout" }
                                            }
                                        }
                                        td class="px-3 py-1 text-gray-800 dark:text-gray-200 break-all" { (entry.message) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn env_vars_panel(
    module_type: ModuleType,
    name: &str,
    vars: &HashMap<String, String>,
) -> Markup {
    let type_path = match module_type {
        ModuleType::Command => "commands",
        ModuleType::Projector => "projectors",
        ModuleType::Effect => "effects",
    };
    let panel_id = format!("env-panel-{type_path}-{name}");
    let post_url = format!("/ui/{type_path}/{name}/env");

    let mut sorted_vars: Vec<(&str, &str)> =
        vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    sorted_vars.sort_by_key(|(k, _)| *k);

    html! {
        div id=(panel_id) {
            p class="text-sm text-amber-700 dark:text-amber-400 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded-md px-3 py-2 mb-4" {
                "Changes take effect on the next module restart."
            }
            @if sorted_vars.is_empty() {
                p class="text-sm text-gray-400 dark:text-gray-500 italic mb-4" { "no environment variables set" }
            } @else {
                div class="overflow-hidden rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 mb-4" {
                    table class="w-full text-sm" {
                        thead {
                            tr class="bg-gray-50 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700" {
                                th class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider w-1/3" { "Key" }
                                th class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider" { "Value" }
                                th class="px-4 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider w-32" { }
                            }
                        }
                        tbody {
                            @for (key, value) in &sorted_vars {
                                tr class="border-b border-gray-100 dark:border-gray-700/50 last:border-0" {
                                    td class="px-4 py-3 font-mono text-xs text-gray-800 dark:text-gray-200 font-medium" { (key) }
                                    td class="px-4 py-3 font-mono text-xs text-gray-600 dark:text-gray-400" {
                                        span data-value=(value) { "••••••••" }
                                        " "
                                        button
                                            type="button"
                                            class="text-xs text-indigo-500 hover:text-indigo-700 ml-1"
                                            onclick="const s=this.previousElementSibling;s.textContent=s.textContent==='••••••••'?s.dataset.value:'••••••••';this.textContent=this.textContent==='Reveal'?'Hide':'Reveal';"
                                            { "Reveal" }
                                    }
                                    td class="px-4 py-3 text-right" {
                                        button
                                            type="button"
                                            class="text-xs text-red-500 hover:text-red-700"
                                            hx-delete={(post_url) "/" (key)}
                                            hx-target={"#" (panel_id)}
                                            hx-swap="outerHTML"
                                            hx-confirm={"Delete " (key) "?"}
                                            { "Delete" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            details class="group" {
                summary class="cursor-pointer text-sm font-medium text-gray-700 dark:text-gray-300 hover:text-gray-900 dark:hover:text-gray-100 select-none list-none flex items-center gap-1 mb-3" {
                    span class="text-gray-400 dark:text-gray-500 group-open:rotate-90 transition-transform inline-block" { "▶" }
                    "Add Variable"
                }
                form
                    hx-post=(post_url)
                    hx-target={"#" (panel_id)}
                    hx-swap="outerHTML"
                    class="flex flex-col gap-3 pl-4"
                {
                    div class="flex gap-3" {
                        input
                            type="text"
                            name="key"
                            placeholder="KEY"
                            required
                            class="w-1/3 rounded-md border border-gray-300 dark:border-gray-600 px-3 py-1.5 text-sm font-mono dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                        input
                            type="text"
                            name="value"
                            placeholder="value"
                            class="flex-1 rounded-md border border-gray-300 dark:border-gray-600 px-3 py-1.5 text-sm font-mono dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                    }
                    button
                        type="submit"
                        class="self-start inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
                        { "Add Variable" }
                }
            }
        }
    }
}

pub fn upload_form(module_type: ModuleType, name: Option<&str>) -> Markup {
    let module_type_str = match module_type {
        ModuleType::Command => "commands",
        ModuleType::Projector => "projectors",
        ModuleType::Effect => "effects",
    };
    let modal_id = match name {
        Some(n) => format!("upload-modal-{module_type_str}-{n}"),
        None => format!("upload-modal-{module_type_str}"),
    };

    html! {
        button
            type="button"
            onclick={"document.getElementById('" (modal_id) "').showModal()"}
            class="mt-4 inline-flex items-center gap-2 px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
        {
            "↑ Upload New Version"
        }

        dialog
            id=(modal_id)
            onclick="if(event.target===this)this.close()"
            class="rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 shadow-xl p-0 w-full max-w-md backdrop:bg-black/40 open:flex open:flex-col"
        {
            div class="flex items-center justify-between px-5 py-4 border-b border-gray-100 dark:border-gray-700/50" {
                h3 class="text-base font-semibold text-gray-800 dark:text-gray-200 m-0" { "Upload New Version" }
                button
                    type="button"
                    onclick={"document.getElementById('" (modal_id) "').close()"}
                    class="text-gray-400 dark:text-gray-500 hover:text-gray-600 dark:hover:text-gray-400 text-xl leading-none"
                { "×" }
            }
            form
                hx-post={"/ui/upload/" (module_type_str)}
                hx-encoding="multipart/form-data"
                hx-target={"#" (modal_id) "-status"}
                hx-swap="innerHTML"
                class="flex flex-col gap-4 px-5 py-5"
            {
                @if let Some(n) = name {
                    input type="hidden" name="name" value=(n);
                } @else {
                    label class="flex flex-col gap-1 text-sm font-medium text-gray-700 dark:text-gray-300" {
                        "Name"
                        input type="text" name="name" required placeholder="module-name"
                            class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                    }
                }
                label class="flex flex-col gap-1 text-sm font-medium text-gray-700 dark:text-gray-300" {
                    "Version"
                    input type="text" name="version" required placeholder="1.0.0"
                        class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                }
                label class="flex flex-col gap-1 text-sm font-medium text-gray-700 dark:text-gray-300" {
                    "WASM file"
                    input type="file" name="wasm" accept=".wasm" required
                        class="block w-full text-sm text-gray-700 dark:text-gray-300";
                }
                label class="flex items-center gap-2 text-sm text-gray-700 dark:text-gray-300" {
                    input type="checkbox" name="activate" value="true";
                    "Activate immediately"
                }
                div class="flex justify-end gap-2 pt-1" {
                    button
                        type="button"
                        onclick={"document.getElementById('" (modal_id) "').close()"}
                        class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                    { "Cancel" }
                    button type="submit"
                        class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
                    { "Upload" }
                }
            }
            div id={"" (modal_id) "-status"} class="px-5 pb-4 empty:hidden" {}
        }
    }
}

enum UnionFieldType {
    Text,
    Number { integer: bool },
    Checkbox,
    CommaSeparated { integer: bool },
}

struct UnionField {
    key: String,
    label: String,
    field_type: UnionFieldType,
    min: Option<f64>,
    max: Option<f64>,
}

struct UnionVariant {
    tag_value: String,
    label: String,
    fields: Vec<UnionField>,
}

enum InputType {
    Text,
    Email,
    Date,
    DateTime,
    Number { integer: bool },
    Checkbox,
    Select(Vec<String>),
    Json,
    DiscriminatedUnion(Vec<UnionVariant>),
}

struct FormField {
    key: String,
    label: String,
    input_type: InputType,
    required: bool,
    description: Option<String>,
    placeholder: Option<&'static str>,
    min: Option<f64>,
    max: Option<f64>,
    min_length: Option<u64>,
    max_length: Option<u64>,
    pattern: Option<String>,
}

fn to_title_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_union_field(key: &str, prop: &serde_json::Value) -> Option<UnionField> {
    let field_type = if prop.get("type").and_then(|t| t.as_str()) == Some("array") {
        let item_type = prop
            .get("items")
            .and_then(|i| i.get("type"))
            .and_then(|t| t.as_str())
            .unwrap_or("string");
        UnionFieldType::CommaSeparated {
            integer: item_type == "integer",
        }
    } else {
        // handle ["string", "number"] (e.g. Decimal)
        let type_str = if let Some(arr) = prop.get("type").and_then(|t| t.as_array()) {
            let non_null: Vec<&str> = arr
                .iter()
                .filter_map(|x| x.as_str())
                .filter(|&s| s != "null")
                .collect();
            if non_null.contains(&"string") {
                "string"
            } else {
                non_null.first().copied().unwrap_or("string")
            }
        } else {
            prop.get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("string")
        };
        match type_str {
            "string" => UnionFieldType::Text,
            "integer" => UnionFieldType::Number { integer: true },
            "number" => UnionFieldType::Number { integer: false },
            "boolean" => UnionFieldType::Checkbox,
            _ => return None,
        }
    };
    Some(UnionField {
        key: key.to_owned(),
        label: to_title_case(key),
        field_type,
        min: prop.get("minimum").and_then(|m| m.as_f64()),
        max: prop.get("maximum").and_then(|m| m.as_f64()),
    })
}

fn parse_discriminated_union(prop: &serde_json::Value) -> Option<Vec<UnionVariant>> {
    let variants_json = prop.get("oneOf").and_then(|v| v.as_array())?;
    if variants_json.is_empty() {
        return None;
    }
    let mut variants = Vec::new();
    for variant in variants_json {
        let properties = variant.get("properties")?.as_object()?;
        let tag_value = properties.get("type")?.get("const")?.as_str()?.to_owned();
        let required_arr: Vec<&str> = variant
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_str()).collect())
            .unwrap_or_default();
        let mut fields = Vec::new();
        for (field_key, field_prop) in properties {
            if field_key == "type" {
                continue;
            }
            if let Some(field) = parse_union_field(field_key, field_prop) {
                // only add fields that are required or have simple types
                let _ = required_arr.contains(&field_key.as_str());
                fields.push(field);
            }
        }
        variants.push(UnionVariant {
            label: to_title_case(&tag_value),
            tag_value,
            fields,
        });
    }
    Some(variants)
}

fn resolve_ref<'a>(ref_str: &str, root: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
    let path = ref_str.strip_prefix("#/")?;
    let mut current = root;
    for part in path.split('/') {
        current = current.get(part)?;
    }
    Some(current)
}

fn parse_fields(schema: &Schema) -> Option<Vec<FormField>> {
    let v: &serde_json::Value = schema.as_value();

    // must be an object type at top level
    if v.get("type").and_then(|t| t.as_str()) != Some("object") {
        return None;
    }

    let properties = v.get("properties")?.as_object()?;
    let required_arr: Vec<&str> = v
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x: &serde_json::Value| x.as_str())
                .collect()
        })
        .unwrap_or_default();

    let mut fields = Vec::new();

    for (key, prop) in properties {
        // resolve $ref before further processing
        let prop = if let Some(ref_str) = prop.get("$ref").and_then(|r| r.as_str()) {
            resolve_ref(ref_str, v).unwrap_or(prop)
        } else {
            prop
        };

        // complex schemas: try discriminated union first, fall back to inline JSON
        if prop.get("anyOf").is_some() || prop.get("oneOf").is_some() || prop.get("allOf").is_some()
        {
            let required = required_arr.contains(&key.as_str());
            let description = prop
                .get("description")
                .and_then(|d| d.as_str())
                .map(|s| s.to_owned());
            let input_type = parse_discriminated_union(prop)
                .map(InputType::DiscriminatedUnion)
                .unwrap_or(InputType::Json);
            fields.push(FormField {
                label: to_title_case(key),
                key: key.clone(),
                input_type,
                required,
                description,
                placeholder: None,
                min: None,
                max: None,
                min_length: None,
                max_length: None,
                pattern: None,
            });
            continue;
        }

        let required = required_arr.contains(&key.as_str());

        // handle nullable: type = ["X", "null"]
        let type_str = if let Some(type_arr) = prop.get("type").and_then(|t| t.as_array()) {
            let non_null: Vec<&str> = type_arr
                .iter()
                .filter_map(|x: &serde_json::Value| x.as_str())
                .filter(|&s| s != "null")
                .collect();
            if non_null.len() == 1 {
                non_null[0]
            } else if non_null.contains(&"string") && non_null.contains(&"number") {
                "string"
            } else {
                if required {
                    return None;
                }
                continue;
            }
        } else if let Some(s) = prop.get("type").and_then(|t| t.as_str()) {
            s
        } else {
            if required {
                return None;
            }
            continue;
        };

        let format = prop.get("format").and_then(|f| f.as_str());
        let enum_vals = prop.get("enum").and_then(|e| e.as_array());

        let input_type = if type_str == "string"
            && let Some(enum_vals) = enum_vals
        {
            let values: Vec<String> = enum_vals
                .iter()
                .filter_map(|val: &serde_json::Value| val.as_str().map(|s| s.to_owned()))
                .collect();
            InputType::Select(values)
        } else {
            match (type_str, format) {
                ("string", Some("email")) => InputType::Email,
                ("string", Some("date")) => InputType::Date,
                ("string", Some("date-time")) => InputType::DateTime,
                ("string", _) => InputType::Text,
                ("integer", _) => InputType::Number { integer: true },
                ("number", _) => InputType::Number { integer: false },
                ("boolean", _) => InputType::Checkbox,
                ("object", _) | ("array", _) => {
                    if required {
                        return None;
                    }
                    continue;
                }
                _ => {
                    if required {
                        return None;
                    }
                    continue;
                }
            }
        };

        let description = prop
            .get("description")
            .and_then(|d| d.as_str())
            .map(|s| s.to_owned());
        let min = prop.get("minimum").and_then(|m| m.as_f64());
        let max = prop.get("maximum").and_then(|m| m.as_f64());
        let min_length = prop.get("minLength").and_then(|m| m.as_u64());
        let max_length = prop.get("maxLength").and_then(|m| m.as_u64());
        let pattern = prop
            .get("pattern")
            .and_then(|p| p.as_str())
            .map(|s| s.to_owned())
            .or_else(|| {
                if format == Some("uuid") {
                    Some("[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}".to_owned())
                } else {
                    None
                }
            });
        let placeholder = match format {
            Some("uuid") => Some("xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"),
            Some("email") => Some("user@example.com"),
            Some("uri") | Some("uri-reference") | Some("iri") => Some("https://example.com"),
            Some("ipv4") => Some("0.0.0.0"),
            Some("ipv6") => Some("::1"),
            Some("hostname") | Some("idn-hostname") => Some("example.com"),
            Some("regex") => Some("^.*$"),
            _ => None,
        };

        fields.push(FormField {
            label: to_title_case(key),
            key: key.clone(),
            input_type,
            required,
            description,
            placeholder,
            min,
            max,
            min_length,
            max_length,
            pattern,
        });
    }

    Some(fields)
}

pub fn execute_form(name: &str, schema: Option<&Schema>) -> Markup {
    let fields = schema.and_then(parse_fields);

    if let Some(fields) = fields {
        let form_id = format!("exec-{name}");
        let execute_url = format!("/ui/commands/{name}/execute");
        let fn_name = name.replace('-', "_");
        let fields_div_id = format!("exec-fields-{name}");
        let raw_div_id = format!("exec-raw-{name}");
        let raw_textarea_id = format!("exec-raw-ta-{name}");
        let raw_toggle_id = format!("exec-raw-toggle-{name}");
        let schema_div_id = format!("exec-schema-{name}");
        let schema_json = schema
            .and_then(|s| serde_json::to_string_pretty(s).ok())
            .unwrap_or_default();
        html! {
            section class="bg-white dark:bg-gray-900 rounded-lg border border-gray-200 dark:border-gray-700 p-5 mt-6" {
                h3 class="text-base font-semibold text-gray-700 dark:text-gray-300 mb-3 mt-0" { "Execute Command" }
                form id=(form_id) class="flex flex-col gap-4" {
                    input type="hidden" name="payload";
                    div id=(fields_div_id) class="flex flex-col gap-4" {
                        @for field in &fields {
                            @if let InputType::DiscriminatedUnion(variants) = &field.input_type {
                                div class="flex flex-col gap-2 rounded-md border border-gray-200 dark:border-gray-700 p-3" data-union-container {
                                    div class="flex flex-col gap-1" {
                                        span class="text-sm font-medium text-gray-700 dark:text-gray-300" {
                                            (field.label)
                                            @if field.required {
                                                span class="text-red-500 ml-1" { "*" }
                                            }
                                        }
                                        @if let Some(desc) = &field.description {
                                            span class="text-gray-400 dark:text-gray-500 text-xs" { (desc) }
                                        }
                                    }
                                    select
                                        data-field=(field.key)
                                        data-type="union"
                                        onchange="umariUnionChange(this)"
                                        class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                                    {
                                        @for variant in variants {
                                            option value=(variant.tag_value) { (variant.label) }
                                        }
                                    }
                                    @for (vi, variant) in variants.iter().enumerate() {
                                        @let hidden = vi != 0;
                                        div
                                            data-union-variant=(variant.tag_value)
                                            class=(if hidden { "flex flex-col gap-2 hidden" } else { "flex flex-col gap-2" })
                                        {
                                            @for vfield in &variant.fields {
                                                label class="flex flex-col gap-1 text-sm font-medium text-gray-700 dark:text-gray-300" {
                                                    (vfield.label)
                                                    @match &vfield.field_type {
                                                        UnionFieldType::Text => {
                                                            input type="text"
                                                                data-union-parent=(field.key)
                                                                data-union-key=(vfield.key)
                                                                data-type="string"
                                                                min=[vfield.min]
                                                                max=[vfield.max]
                                                                disabled[hidden]
                                                                class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                                        }
                                                        UnionFieldType::Number { integer } => {
                                                            input type="number"
                                                                data-union-parent=(field.key)
                                                                data-union-key=(vfield.key)
                                                                data-type=(if *integer { "integer" } else { "number" })
                                                                step=(if *integer { "1" } else { "any" })
                                                                min=[vfield.min]
                                                                max=[vfield.max]
                                                                disabled[hidden]
                                                                class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                                        }
                                                        UnionFieldType::Checkbox => {
                                                            input type="checkbox"
                                                                data-union-parent=(field.key)
                                                                data-union-key=(vfield.key)
                                                                data-type="boolean"
                                                                disabled[hidden]
                                                                class="h-4 w-4 rounded border-gray-300 dark:border-gray-600 text-indigo-600 focus:ring-indigo-500";
                                                        }
                                                        UnionFieldType::CommaSeparated { integer } => {
                                                            input type="text"
                                                                data-union-parent=(field.key)
                                                                data-union-key=(vfield.key)
                                                                data-type=(if *integer { "integers" } else { "numbers" })
                                                                placeholder="1, 2, 3"
                                                                disabled[hidden]
                                                                class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } @else {
                                label class="flex flex-col gap-1 text-sm font-medium text-gray-700 dark:text-gray-300" {
                                    span {
                                        (field.label)
                                        @if field.required {
                                            span class="text-red-500 ml-1" { "*" }
                                        }
                                    }
                                    @if let Some(desc) = &field.description {
                                        span class="text-gray-400 dark:text-gray-500 text-xs font-normal" { (desc) }
                                    }
                                    @match &field.input_type {
                                        InputType::Text => {
                                            input type="text"
                                                name=(field.key)
                                                data-field=(field.key)
                                                data-type="string"
                                                placeholder=[field.placeholder]
                                                required[field.required]
                                                minlength=[field.min_length]
                                                maxlength=[field.max_length]
                                                pattern=[field.pattern.as_deref()]
                                                class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                        }
                                        InputType::Email => {
                                            input type="email"
                                                name=(field.key)
                                                data-field=(field.key)
                                                data-type="string"
                                                placeholder="user@example.com"
                                                required[field.required]
                                                minlength=[field.min_length]
                                                maxlength=[field.max_length]
                                                class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                        }
                                        InputType::Date => {
                                            input type="date"
                                                name=(field.key)
                                                data-field=(field.key)
                                                data-type="string"
                                                required[field.required]
                                                class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                        }
                                        InputType::DateTime => {
                                            input type="datetime-local"
                                                name=(field.key)
                                                data-field=(field.key)
                                                data-type="string"
                                                required[field.required]
                                                class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                        }
                                        InputType::Number { integer } => {
                                            input type="number"
                                                name=(field.key)
                                                data-field=(field.key)
                                                data-type=(if *integer { "integer" } else { "number" })
                                                step=(if *integer { "1" } else { "any" })
                                                min=[field.min]
                                                max=[field.max]
                                                placeholder=[field.placeholder]
                                                required[field.required]
                                                class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                        }
                                        InputType::Checkbox => {
                                            input type="checkbox"
                                                name=(field.key)
                                                data-field=(field.key)
                                                data-type="boolean"
                                                class="h-4 w-4 rounded border-gray-300 dark:border-gray-600 text-indigo-600 focus:ring-indigo-500";
                                        }
                                        InputType::Select(options) => {
                                            select name=(field.key)
                                                data-field=(field.key)
                                                data-type="string"
                                                required[field.required]
                                                class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                                            {
                                                @if !field.required {
                                                    option value="" { "" }
                                                }
                                                @for opt in options {
                                                    option value=(opt) { (opt) }
                                                }
                                            }
                                        }
                                        InputType::Json => {
                                            textarea name=(field.key)
                                                data-field=(field.key)
                                                data-type="json"
                                                required[field.required]
                                                rows="3"
                                                placeholder="{}"
                                                class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm font-mono dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500" {}
                                        }
                                        InputType::DiscriminatedUnion(_) => {}
                                    }
                                }
                            }
                        }
                    }
                    div id=(raw_div_id) class="hidden" {
                        label class="flex flex-col gap-1 text-sm font-medium text-gray-700 dark:text-gray-300" {
                            "JSON Payload"
                            textarea id=(raw_textarea_id) rows="8" placeholder="{}"
                                class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm font-mono dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500" {}
                        }
                    }
                    div class="flex items-center justify-between" {
                        button type="button"
                            onclick={
                                "umariExec_" (fn_name) "(this)"
                            }
                            class="self-start inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
                            { "Execute" }
                        div class="flex items-center gap-4" {
                            label class="flex items-center gap-2 text-xs text-gray-400 dark:text-gray-500 font-normal cursor-pointer" {
                                input type="checkbox" id=(raw_toggle_id)
                                    onchange={"umariToggleRaw_" (fn_name) "(this)"}
                                    class="h-3.5 w-3.5 rounded border-gray-300 dark:border-gray-600 text-indigo-600 focus:ring-indigo-500";
                                "Raw JSON"
                            }
                            label class="flex items-center gap-2 text-xs text-gray-400 dark:text-gray-500 font-normal cursor-pointer" {
                                input type="checkbox" data-bypass-validation
                                    class="h-3.5 w-3.5 rounded border-gray-300 dark:border-gray-600 text-indigo-600 focus:ring-indigo-500";
                                "Bypass validation"
                            }
                            button type="button"
                                onclick={"umariToggleSchema_" (fn_name) "(this)"}
                                class="text-xs text-gray-400 dark:text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 transition-colors"
                                { "View Schema" }
                        }
                    }
                }
                div #execute-result {}
                div id=(schema_div_id) class="hidden mt-4" {
                    div class="flex items-center justify-between mb-1" {
                        span class="text-xs font-medium text-gray-500 dark:text-gray-400" { "JSON Schema" }
                        button type="button"
                            onclick={"umariCopySchema_" (fn_name) "(this)"}
                            class="text-xs text-gray-400 dark:text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 transition-colors"
                            { "Copy" }
                    }
                    pre class="overflow-x-auto rounded-md border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 p-3 text-xs text-gray-700 dark:text-gray-300 font-mono" {
                        (schema_json)
                    }
                }
                script {
                    (PreEscaped(format!(
                        r#"function umariCollectUnion(el, obj) {{
  const key = el.dataset.field;
  const tag = el.value;
  const unionObj = {{ type: tag }};
  const container = el.closest('[data-union-container]');
  if (container) {{
    container.querySelectorAll('[data-union-key]').forEach(subEl => {{
      if (subEl.disabled) return;
      const subKey = subEl.dataset.unionKey;
      const subType = subEl.dataset.type;
      if (subType === 'boolean') {{ unionObj[subKey] = subEl.checked; return; }}
      if (subEl.value === '') return;
      if (subType === 'integers') {{ unionObj[subKey] = subEl.value.split(',').map(s => parseInt(s.trim(), 10)).filter(n => !isNaN(n)); return; }}
      if (subType === 'numbers') {{ unionObj[subKey] = subEl.value.split(',').map(s => parseFloat(s.trim())).filter(n => !isNaN(n)); return; }}
      unionObj[subKey] = subType === 'integer' ? parseInt(subEl.value, 10) : subType === 'number' ? parseFloat(subEl.value) : subEl.value;
    }});
  }}
  obj[key] = unionObj;
}}
function umariUnionChange(select) {{
  const container = select.closest('[data-union-container]');
  const tag = select.value;
  container.querySelectorAll('[data-union-variant]').forEach(div => {{
    const show = div.dataset.unionVariant === tag;
    div.classList.toggle('hidden', !show);
    div.querySelectorAll('input, select, textarea').forEach(el => {{ el.disabled = !show; }});
  }});
}}
function umariExec_{fn_name}(btn) {{
  const form = btn.closest('form');
  const rawMode = form.querySelector('#{raw_toggle_id}')?.checked;
  if (rawMode) {{
    const raw = document.getElementById('{raw_textarea_id}').value || '{{}}';
    htmx.ajax('POST', '{execute_url}', {{
      target: '#execute-result', swap: 'innerHTML',
      values: {{ payload: raw }}
    }});
    return;
  }}
  const bypass = form.querySelector('[data-bypass-validation]')?.checked;
  if (!bypass && !form.reportValidity()) return;
  const obj = {{}};
  form.querySelectorAll('[data-field]').forEach(el => {{
    const key = el.dataset.field;
    const type = el.dataset.type;
    if (type === 'boolean') {{ obj[key] = el.checked; return; }}
    if (type === 'union') {{ umariCollectUnion(el, obj); return; }}
    if (el.value === '') return;
    if (type === 'json') {{ try {{ obj[key] = JSON.parse(el.value); }} catch(e) {{}} return; }}
    obj[key] = (type === 'integer') ? parseInt(el.value, 10)
             : (type === 'number')  ? parseFloat(el.value)
             : el.value;
  }});
  htmx.ajax('POST', '{execute_url}', {{
    target: '#execute-result',
    swap: 'innerHTML',
    values: {{ payload: JSON.stringify(obj) }}
  }});
}}
function umariToggleRaw_{fn_name}(checkbox) {{
  const form = checkbox.closest('form');
  const fieldsDiv = document.getElementById('{fields_div_id}');
  const rawDiv = document.getElementById('{raw_div_id}');
  const ta = document.getElementById('{raw_textarea_id}');
  if (checkbox.checked) {{
    const obj = {{}};
    form.querySelectorAll('[data-field]').forEach(el => {{
      const key = el.dataset.field;
      const type = el.dataset.type;
      if (type === 'boolean') {{ obj[key] = el.checked; return; }}
      if (type === 'union') {{ umariCollectUnion(el, obj); return; }}
      if (el.value === '') return;
      if (type === 'json') {{ try {{ obj[key] = JSON.parse(el.value); }} catch(err) {{}} return; }}
      obj[key] = (type === 'integer') ? parseInt(el.value, 10)
               : (type === 'number')  ? parseFloat(el.value)
               : el.value;
    }});
    ta.value = JSON.stringify(obj, null, 2);
    fieldsDiv.classList.add('hidden');
    rawDiv.classList.remove('hidden');
  }} else {{
    fieldsDiv.classList.remove('hidden');
    rawDiv.classList.add('hidden');
  }}
}}
function umariToggleSchema_{fn_name}(btn) {{
  const schemaDiv = document.getElementById('{schema_div_id}');
  const hidden = schemaDiv.classList.toggle('hidden');
  btn.textContent = hidden ? 'View Schema' : 'Hide Schema';
}}
function umariCopySchema_{fn_name}(btn) {{
  const schemaDiv = document.getElementById('{schema_div_id}');
  const text = schemaDiv.querySelector('pre').textContent;
  navigator.clipboard.writeText(text).then(() => {{
    const orig = btn.textContent;
    btn.textContent = 'Copied!';
    setTimeout(() => {{ btn.textContent = orig; }}, 1500);
  }});
}}"#
                    )))
                }
            }
        }
    } else {
        html! {
            section class="bg-white dark:bg-gray-900 rounded-lg border border-gray-200 dark:border-gray-700 p-5 mt-6" {
                h3 class="text-base font-semibold text-gray-700 dark:text-gray-300 mb-3 mt-0" { "Execute Command" }
                form
                    hx-post={"/ui/commands/" (name) "/execute"}
                    hx-target="#execute-result"
                    hx-swap="innerHTML"
                    class="flex flex-col gap-4"
                {
                    label class="flex flex-col gap-1 text-sm font-medium text-gray-700 dark:text-gray-300" {
                        "JSON Payload"
                        textarea name="payload" rows="6" placeholder="{}"
                            class="block w-full rounded-md border border-gray-300 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                            {}
                    }
                    button type="submit"
                        class="self-start inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
                        { "Execute" }
                }
                div #execute-result {}
            }
        }
    }
}

pub fn sql_query_section(query_url: &str, default_query: Option<&str>) -> Markup {
    let placeholder = default_query.unwrap_or("SELECT * FROM ...");
    html! {
        section {
            @if default_query.is_none() {
                p class="text-sm text-gray-400 italic mb-3" { "no database found" }
            }
            form
                hx-post=(query_url)
                hx-target="#sql-results"
                hx-swap="innerHTML"
                class="flex flex-col gap-2"
            {
                textarea
                    name="sql"
                    rows="3"
                    placeholder=(placeholder)
                    class="block w-full rounded-md border border-gray-300 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 dark:placeholder-gray-500 px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                    { (default_query.unwrap_or("")) }
                button type="submit"
                    class="self-start inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
                    { "Run" }
            }
            div id="sql-results" class="mt-3" {}
        }
    }
}

/// Returns the default SELECT query for a module's SQLite database (the first non-meta table).
pub async fn default_sql_query(db_path: PathBuf) -> Option<String> {
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()?;
        conn.query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name != 'module_meta' ORDER BY name LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(|table| format!("SELECT * FROM {table} LIMIT 100"))
    })
    .await
    .unwrap_or(None)
}

/// Executes a read-only SQL query against a module SQLite database and returns an HTML result table.
pub async fn run_sql_query(db_path: PathBuf, sql: String, module_label: &'static str) -> Markup {
    let sql = sql.trim().to_string();
    if !sql.to_ascii_lowercase().starts_with("select") {
        return html! {
            div class="rounded-md bg-red-50 border border-red-200 p-4 text-sm text-red-800" {
                p class="font-semibold mb-1" { "Error" }
                p { "only SELECT queries are allowed" }
            }
        };
    }
    if !db_path.exists() {
        return html! {
            div class="rounded-md bg-red-50 border border-red-200 p-4 text-sm text-red-800" {
                p class="font-semibold mb-1" { "Error" }
                p { "no database found for this " (module_label) }
            }
        };
    }

    let err_html = |msg: String| {
        html! {
            div class="rounded-md bg-red-50 border border-red-200 p-4 text-sm text-red-800" {
                p class="font-semibold mb-1" { "Error" }
                p { (msg) }
            }
        }
    };

    let result = tokio::task::spawn_blocking(move || -> Result<Markup, String> {
        let conn = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|err| err.to_string())?;

        let mut stmt = conn.prepare(&sql).map_err(|err| err.to_string())?;
        let column_names: Vec<String> =
            stmt.column_names().iter().map(|s| s.to_string()).collect();

        let rows: Vec<Vec<String>> = stmt
            .query_map([], |row| {
                let values = (0..column_names.len())
                    .map(|i| {
                        row.get_ref(i)
                            .map(|v| match v {
                                rusqlite::types::ValueRef::Null => "NULL".to_string(),
                                rusqlite::types::ValueRef::Integer(n) => n.to_string(),
                                rusqlite::types::ValueRef::Real(n) => n.to_string(),
                                rusqlite::types::ValueRef::Text(s) => {
                                    String::from_utf8_lossy(s).into_owned()
                                }
                                rusqlite::types::ValueRef::Blob(b) => {
                                    format!("<blob {} bytes>", b.len())
                                }
                            })
                            .unwrap_or_else(|_| "?".to_string())
                    })
                    .collect();
                Ok(values)
            })
            .map_err(|err| err.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|err: rusqlite::Error| err.to_string())?;

        Ok(html! {
            @if rows.is_empty() {
                p class="text-sm text-gray-400 italic" { "no rows returned" }
            } @else {
                div class="overflow-x-auto overflow-hidden rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900" {
                    table class="w-full text-xs font-mono" {
                        thead {
                            tr class="bg-gray-50 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700" {
                                @for col in &column_names {
                                    th class="px-3 py-2 text-left font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider whitespace-nowrap" { (col) }
                                }
                            }
                        }
                        tbody {
                            @for row in &rows {
                                tr class="border-b border-gray-100 dark:border-gray-700/50 last:border-0 hover:bg-gray-50 dark:hover:bg-gray-800" {
                                    @for cell in row {
                                        td class="px-3 py-1.5 text-gray-800 dark:text-gray-200 whitespace-nowrap" { (cell) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    })
    .await;

    match result {
        Ok(Ok(markup)) => markup,
        Ok(Err(msg)) => err_html(msg),
        Err(err) => err_html(err.to_string()),
    }
}

/// The JSON syntax highlighter reused from the events view, applied to `pre.ev-json`.
const JSON_HIGHLIGHT_JS: &str = r#"
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
"#;

const PROJECTION_EDITOR_JS: &str = r#"
(function() {
  var VER = '5.65.16';
  var BASE = 'https://cdnjs.cloudflare.com/ajax/libs/codemirror/' + VER + '/';
  var LS = 'umari-projections';
  function store() { try { return JSON.parse(localStorage.getItem(LS)) || {}; } catch (e) { return {}; } }
  function persist(s) { localStorage.setItem(LS, JSON.stringify(s)); }
  function css(href) {
    if (document.querySelector('link[href="' + href + '"]')) return;
    var l = document.createElement('link'); l.rel = 'stylesheet'; l.href = href;
    document.head.appendChild(l);
  }
  function js(src, cb) {
    var ex = document.querySelector('script[src="' + src + '"]');
    if (ex) { if (ex.__loaded) cb(); else ex.addEventListener('load', cb); return; }
    var s = document.createElement('script'); s.src = src;
    s.addEventListener('load', function() { s.__loaded = true; cb(); });
    document.head.appendChild(s);
  }
  function refresh(sel) {
    var s = store(); var cur = sel.value;
    sel.innerHTML = '';
    var o = document.createElement('option'); o.value = ''; o.textContent = 'Saved projections…';
    sel.appendChild(o);
    Object.keys(s).sort().forEach(function(name) {
      var op = document.createElement('option'); op.value = name; op.textContent = name;
      sel.appendChild(op);
    });
    sel.value = cur;
  }
  function init() {
    var ta = document.getElementById('projection-script');
    if (!ta || ta.__cm) return;
    ta.__cm = true;
    var dark = document.documentElement.classList.contains('dark');
    var cm = CodeMirror.fromTextArea(ta, {
      mode: 'javascript', lineNumbers: true, tabSize: 2, indentUnit: 2,
      lineWrapping: false, theme: dark ? 'material-darker' : 'default'
    });
    cm.setSize(null, 380);
    window.__umariCM = cm;
    if (!window.__umariCMHooked) {
      window.__umariCMHooked = true;
      document.body.addEventListener('htmx:configRequest', function() {
        if (window.__umariCM) window.__umariCM.save();
      });
    }
    var sel = document.getElementById('projection-saved');
    refresh(sel);
    sel.addEventListener('change', function() {
      var s = store();
      if (sel.value && s[sel.value] != null) cm.setValue(s[sel.value]);
    });
    document.getElementById('projection-save').addEventListener('click', function() {
      var name = window.prompt('Save projection as:', sel.value || '');
      if (!name || !(name = name.trim())) return;
      var s = store(); s[name] = cm.getValue(); persist(s);
      refresh(sel); sel.value = name;
    });
    document.getElementById('projection-delete').addEventListener('click', function() {
      if (!sel.value) return;
      if (!window.confirm('Delete saved projection "' + sel.value + '"?')) return;
      var s = store(); delete s[sel.value]; persist(s);
      refresh(sel); sel.value = '';
    });
  }
  function boot() {
    if (window.CodeMirror) { init(); return; }
    css(BASE + 'codemirror.min.css');
    css(BASE + 'theme/material-darker.min.css');
    js(BASE + 'codemirror.min.js', function() {
      js(BASE + 'mode/javascript/javascript.min.js', init);
    });
  }
  boot();
})();
"#;

/// The editor + run form for the Explore (in-memory projection) page.
pub fn projection_editor(run_url: &str, starter_script: &str) -> Markup {
    let toolbar_btn = "px-2.5 py-1.5 text-xs font-medium rounded-md border border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors";
    html! {
        section class="flex flex-col gap-4" {
            form
                hx-post=(run_url)
                hx-target="#projection-results"
                hx-swap="innerHTML"
                hx-indicator="#projection-progress"
                hx-disabled-elt="#projection-run"
                class="flex flex-col gap-3"
            {
                div class="flex items-center gap-2" {
                    select id="projection-saved"
                        class="text-xs rounded-md border border-gray-300 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 px-2 py-1.5 focus:outline-none focus:ring-2 focus:ring-indigo-300"
                    {
                        option value="" { "Saved projections…" }
                    }
                    button type="button" id="projection-save" class=(toolbar_btn) { "Save" }
                    button type="button" id="projection-delete" class=(toolbar_btn) { "Delete" }
                }
                textarea
                    id="projection-script"
                    name="script"
                    rows="18"
                    spellcheck="false"
                    class="block w-full rounded-md border border-gray-300 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 px-3 py-2 text-xs font-mono leading-relaxed focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                    { (starter_script) }
                div class="flex items-center gap-4" {
                    button type="submit" id="projection-run"
                        class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                        { "Run projection" }
                    label class="text-xs text-gray-500 dark:text-gray-400 flex items-center gap-1.5" {
                        "Limit"
                        input type="number" name="limit" min="1" placeholder="all"
                            class="w-24 border border-gray-300 dark:border-gray-600 rounded px-2 py-1 text-xs dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-300";
                    }
                }
                div id="projection-progress" class="htmx-indicator flex items-center gap-3" {
                    div class="relative h-1 flex-1 overflow-hidden rounded-full bg-indigo-100 dark:bg-indigo-500/20" {
                        div class="umari-bar" {}
                    }
                    span class="text-xs text-gray-500 dark:text-gray-400 whitespace-nowrap" { "Running…" }
                }
            }
            div id="projection-results" {}
        }
        script { (PreEscaped(PROJECTION_EDITOR_JS)) }
    }
}

/// Renders a red error box for a failed projection run.
pub fn projection_error(message: &str) -> Markup {
    html! {
        div class="rounded-md bg-red-50 border border-red-200 p-4 text-sm text-red-800" {
            p class="font-semibold mb-1" { "Error" }
            p class="whitespace-pre-wrap font-mono text-xs" { (message) }
        }
    }
}

/// Renders a projection's JSON result: an array of flat objects becomes a table;
/// any other value is shown as pretty-printed JSON. Captured logs follow below.
pub fn projection_result(result: &Value, logs: &[String]) -> Markup {
    let body = match result {
        Value::Array(items) => projection_rows(items),
        other => projection_json(other),
    };
    html! {
        (body)
        @if !logs.is_empty() {
            div class="mt-4" {
                p class="text-xs font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-wider mb-1" { "console" }
                pre class="text-xs font-mono text-gray-600 dark:text-gray-400 whitespace-pre-wrap bg-gray-50 dark:bg-gray-800 rounded-md p-3" {
                    @for line in logs { (line) "\n" }
                }
            }
        }
    }
}

fn projection_rows(items: &[Value]) -> Markup {
    if items.is_empty() {
        return html! { p class="text-sm text-gray-400 italic" { "no rows returned" } };
    }
    let (columns, rows): (Vec<String>, Vec<Vec<String>>) = if items.iter().all(Value::is_object) {
        let mut columns: Vec<String> = Vec::new();
        for item in items {
            if let Value::Object(map) = item {
                for key in map.keys() {
                    if !columns.contains(key) {
                        columns.push(key.clone());
                    }
                }
            }
        }
        let rows = items
            .iter()
            .map(|item| columns.iter().map(|col| cell_text(item.get(col))).collect())
            .collect();
        (columns, rows)
    } else {
        let rows = items
            .iter()
            .map(|item| vec![cell_text(Some(item))])
            .collect();
        (vec!["value".to_string()], rows)
    };

    html! {
        div class="overflow-x-auto overflow-hidden rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900" {
            table class="w-full text-xs font-mono" {
                thead {
                    tr class="bg-gray-50 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700" {
                        @for col in &columns {
                            th class="px-3 py-2 text-left font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider whitespace-nowrap" { (col) }
                        }
                    }
                }
                tbody {
                    @for row in &rows {
                        tr class="border-b border-gray-100 dark:border-gray-700/50 last:border-0 hover:bg-gray-50 dark:hover:bg-gray-800" {
                            @for cell in row {
                                td class="px-3 py-1.5 text-gray-800 dark:text-gray-200 whitespace-nowrap" { (cell) }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn projection_json(value: &Value) -> Markup {
    let pretty = serde_json::to_string_pretty(value).unwrap_or_default();
    html! {
        pre class="ev-json text-xs text-gray-800 dark:text-gray-200 whitespace-pre-wrap break-all bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-lg px-4 py-3" {
            (pretty)
        }
        script { (PreEscaped(JSON_HIGHLIGHT_JS)) }
    }
}

/// One table cell: strings verbatim, scalars stringified, null/missing blank, and
/// nested values as compact JSON.
fn cell_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(other) => other.to_string(),
    }
}
