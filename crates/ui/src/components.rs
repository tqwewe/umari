use std::{collections::HashMap, sync::Arc};

use maud::{Markup, PreEscaped, html};
use schemars::Schema;
use semver::Version;
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
        ModuleType::Policy => "policies",
        ModuleType::Projector => "projectors",
        ModuleType::Effect => "effects",
    };

    html! {
        div class="overflow-hidden rounded-lg border border-gray-200 bg-white" {
            table class="w-full text-sm" {
                thead {
                    tr class="bg-gray-50 border-b border-gray-200" {
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "Name" }
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "Active Version" }
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "Status" }
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "Position" }
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "SHA256" }
                    }
                }
                tbody {
                    @if names.is_empty() {
                        tr {
                            td colspan="5" class="px-4 py-3 text-sm text-gray-500" { "No modules uploaded yet." }
                        }
                    }
                    @for name in names {
                        @let active = active_modules.iter().find(|m| m.name == *name);
                        @let module_health = health.get(name.as_str());
                        tr class="border-b border-gray-100 last:border-0 hover:bg-gray-50" {
                            td class="px-4 py-3" {
                                a href={"/ui/" (type_path) "/" (name)}
                                    hx-get={"/ui/" (type_path) "/" (name)}
                                    hx-target="#content"
                                    hx-push-url={"/ui/" (type_path) "/" (name)}
                                    class="text-indigo-600 hover:text-indigo-800 font-medium"
                                    { (name) }
                            }
                            td class="px-4 py-3 text-gray-700" {
                                @if let Some(a) = active {
                                    span class="text-emerald-600 font-medium" { (a.version) }
                                } @else {
                                    span class="text-gray-400" { "—" }
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
                            td class="px-4 py-3 text-gray-500 font-mono text-xs" {
                                @if let Some(pos) = module_health.and_then(|h| h.last_position) {
                                    (pos)
                                }
                            }
                            td class="px-4 py-3 text-gray-500 font-mono text-xs" {
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
        ModuleType::Policy => "policies",
        ModuleType::Projector => "projectors",
        ModuleType::Effect => "effects",
    };
    let replay_url = format!("/ui/{type_path}/{name}/replay");

    html! {
        div class="rounded-lg border border-gray-200 bg-white p-4 flex items-start justify-between gap-4" {
            div class="flex items-center gap-8" {
                div {
                    p class="text-xs font-medium text-gray-500 uppercase tracking-wider mb-1" { "Status" }
                    @if active_version.is_none() {
                        span class="text-gray-400 text-sm" { "Inactive" }
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
                    p class="text-xs font-medium text-gray-500 uppercase tracking-wider mb-1" { "Active Version" }
                    @if let Some(v) = active_version {
                        span class="text-emerald-600 font-mono text-sm font-medium" { (v) }
                    } @else {
                        span class="text-gray-400 text-sm" { "—" }
                    }
                }
                div {
                    p class="text-xs font-medium text-gray-500 uppercase tracking-wider mb-1" { "Position" }
                    @if let Some(pos) = health.and_then(|h| h.last_position) {
                        span class="text-gray-700 font-mono text-sm" { (pos) }
                    } @else {
                        span class="text-gray-400 text-sm" { "—" }
                    }
                }
            }
            @if active_version.is_some() {
                div class="flex flex-col items-end gap-1 shrink-0" {
                    dialog
                        id="confirm-reset-replay"
                        onclick="if(event.target===this)this.close()"
                        class="rounded-lg border border-gray-200 shadow-xl backdrop:bg-black/40 p-0 w-full max-w-md"
                    {
                        div class="p-6" {
                            h3 class="text-lg font-semibold text-gray-900 mb-2" { "Reset & Replay" }
                            p class="text-sm text-gray-600 mb-4" {
                                "This will reset the module database and replay all events from position 0. "
                                "Any state built up by this module will be lost."
                            }
                            div class="flex justify-end gap-2" {
                                button
                                    type="button"
                                    onclick="document.getElementById('confirm-reset-replay').close()"
                                    class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-white border border-gray-300 text-gray-700 hover:bg-gray-50 transition-colors"
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
        ModuleType::Policy => "policies",
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
                    class="rounded-lg border border-gray-200 shadow-xl backdrop:bg-black/40 p-0 w-full max-w-md"
                {
                    div class="p-6" {
                        h3 class="text-lg font-semibold text-gray-900 mb-2" { "Major Version Change" }
                        p class="text-sm text-gray-600 mb-4" {
                            "Activating version " strong { (info.version) } " will reset the module database, "
                            "as it has a different major version to the currently active version " strong { (active_ver_str) } "."
                        }
                        div class="flex justify-end gap-2" {
                            button
                                type="button"
                                onclick={"document.getElementById('" (modal_id) "').close()"}
                                class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-white border border-gray-300 text-gray-700 hover:bg-gray-50 transition-colors"
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
        div class="overflow-hidden rounded-lg border border-gray-200 bg-white" {
            table id=(table_id) class="w-full text-sm" {
                thead {
                    tr class="bg-gray-50 border-b border-gray-200" {
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "Version" }
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "Active" }
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "SHA256" }
                        th class="px-4 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider" { "Actions" }
                    }
                }
                tbody {
                    @if versions.is_empty() {
                        tr {
                            td colspan="4" class="px-4 py-3 text-sm text-gray-500" { "No versions uploaded yet." }
                        }
                    }
                    @for info in &versions {
                        @let is_active = active_version.is_some_and(|v| v == &info.version);
                        @let major_differs = !is_active && active_version.is_some_and(|av| av.major != info.version.major);
                        @let sha_short = &info.sha256[..12.min(info.sha256.len())];
                        tr class="border-b border-gray-100 last:border-0 hover:bg-gray-50" {
                            td class="px-4 py-3 text-gray-700 font-mono text-xs" { (info.version) }
                            td class="px-4 py-3" {
                                @if is_active {
                                    span class="text-emerald-500 font-semibold" { "✓" }
                                }
                            }
                            td class="px-4 py-3 text-gray-500 font-mono text-xs" {
                                span class="inline-flex items-center gap-1.5" {
                                    span title=(info.sha256) { (sha_short) "…" }
                                    button
                                        type="button"
                                        title="Copy full SHA256"
                                        onclick={"navigator.clipboard.writeText('" (info.sha256) "').then(() => { const el = this; el.textContent = '✓'; setTimeout(() => el.textContent = '⧉', 1500); })"}
                                        class="text-gray-400 hover:text-gray-600 transition-colors"
                                        { "⧉" }
                                }
                            }
                            td class="px-4 py-3 text-right" {
                                @if is_active {
                                    button
                                        hx-delete={"/ui/" (module_type_str) "/" (name) "/active"}
                                        hx-target={"#" (table_id)}
                                        hx-swap="outerHTML"
                                        class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-white border border-gray-300 text-gray-700 hover:bg-gray-50 transition-colors"
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
                            }
                        }
                    }
                }
            }
        }
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
            div class="flex border-b border-gray-200 mb-6" {
                @for (i, label) in labels.iter().enumerate() {
                    button
                        type="button"
                        data-tab-btn=""
                        data-tab-slug=(slugs[i])
                        onclick=(format!("umariTabs('{}',{},'{}')", id, i, slugs[i]))
                        class=(if i == 0 {
                            "px-4 py-2 text-sm font-medium -mb-px border-b-2 border-indigo-600 text-indigo-600"
                        } else {
                            "px-4 py-2 text-sm font-medium -mb-px border-b-2 border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300"
                        })
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
                    b.classList.toggle('border-indigo-600', i === idx);
                    b.classList.toggle('text-indigo-600', i === idx);
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
                div class="overflow-hidden rounded-lg border border-gray-200 bg-white" {
                    table class="w-full text-xs font-mono" {
                        tbody {
                            @for entry in entries {
                                @let ts = entry.timestamp.format("%H:%M:%S%.3f").to_string();
                                @let is_stderr = matches!(entry.stream, LogStream::Stderr);
                                tr class="border-b border-gray-100 last:border-0" {
                                    td class="px-3 py-1 text-gray-400 whitespace-nowrap w-28" { (ts) }
                                    td class="px-2 py-1 whitespace-nowrap w-16" {
                                        @if is_stderr {
                                            span class="text-red-500 font-semibold" { "stderr" }
                                        } @else {
                                            span class="text-gray-400" { "stdout" }
                                        }
                                    }
                                    td class="px-3 py-1 text-gray-800 break-all" { (entry.message) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn upload_form(module_type: ModuleType, name: Option<&str>) -> Markup {
    let module_type_str = match module_type {
        ModuleType::Command => "commands",
        ModuleType::Policy => "policies",
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
            class="rounded-xl border border-gray-200 shadow-xl p-0 w-full max-w-md backdrop:bg-black/40 open:flex open:flex-col"
        {
            div class="flex items-center justify-between px-5 py-4 border-b border-gray-100" {
                h3 class="text-base font-semibold text-gray-800 m-0" { "Upload New Version" }
                button
                    type="button"
                    onclick={"document.getElementById('" (modal_id) "').close()"}
                    class="text-gray-400 hover:text-gray-600 text-xl leading-none"
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
                    label class="flex flex-col gap-1 text-sm font-medium text-gray-700" {
                        "Name"
                        input type="text" name="name" required placeholder="module-name"
                            class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                    }
                }
                label class="flex flex-col gap-1 text-sm font-medium text-gray-700" {
                    "Version"
                    input type="text" name="version" required placeholder="1.0.0"
                        class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                }
                label class="flex flex-col gap-1 text-sm font-medium text-gray-700" {
                    "WASM file"
                    input type="file" name="wasm" accept=".wasm" required
                        class="block w-full text-sm text-gray-700";
                }
                label class="flex items-center gap-2 text-sm text-gray-700" {
                    input type="checkbox" name="activate" value="true";
                    "Activate immediately"
                }
                div class="flex justify-end gap-2 pt-1" {
                    button
                        type="button"
                        onclick={"document.getElementById('" (modal_id) "').close()"}
                        class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-white border border-gray-300 text-gray-700 hover:bg-gray-50 transition-colors"
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

enum InputType {
    Text,
    Email,
    Date,
    DateTime,
    Number { integer: bool },
    Checkbox,
    Select(Vec<String>),
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
        // reject complex schemas
        if prop.get("anyOf").is_some()
            || prop.get("oneOf").is_some()
            || prop.get("allOf").is_some()
            || prop.get("$ref").is_some()
        {
            if required_arr.contains(&key.as_str()) {
                return None;
            }
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
        html! {
            section class="bg-white rounded-lg border border-gray-200 p-5 mt-6" {
                h3 class="text-base font-semibold text-gray-700 mb-3 mt-0" { "Execute Command" }
                form id=(form_id) class="flex flex-col gap-4" {
                    input type="hidden" name="payload";
                    @for field in &fields {
                        label class="flex flex-col gap-1 text-sm font-medium text-gray-700" {
                            span {
                                (field.label)
                                @if field.required {
                                    span class="text-red-500 ml-1" { "*" }
                                }
                            }
                            @if let Some(desc) = &field.description {
                                span class="text-gray-400 text-xs font-normal" { (desc) }
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
                                        class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
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
                                        class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                }
                                InputType::Date => {
                                    input type="date"
                                        name=(field.key)
                                        data-field=(field.key)
                                        data-type="string"
                                        required[field.required]
                                        class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                }
                                InputType::DateTime => {
                                    input type="datetime-local"
                                        name=(field.key)
                                        data-field=(field.key)
                                        data-type="string"
                                        required[field.required]
                                        class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
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
                                        class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                }
                                InputType::Checkbox => {
                                    input type="checkbox"
                                        name=(field.key)
                                        data-field=(field.key)
                                        data-type="boolean"
                                        class="h-4 w-4 rounded border-gray-300 text-indigo-600 focus:ring-indigo-500";
                                }
                                InputType::Select(options) => {
                                    select name=(field.key)
                                        data-field=(field.key)
                                        data-type="string"
                                        required[field.required]
                                        class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                                    {
                                        @if !field.required {
                                            option value="" { "" }
                                        }
                                        @for opt in options {
                                            option value=(opt) { (opt) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div class="flex items-center justify-between" {
                        button type="button"
                            onclick={
                                "umariExec_" (fn_name) "(this)"
                            }
                            class="self-start inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
                            { "Execute" }
                        label class="flex items-center gap-2 text-xs text-gray-400 font-normal cursor-pointer" {
                            input type="checkbox" data-bypass-validation
                                class="h-3.5 w-3.5 rounded border-gray-300 text-indigo-600 focus:ring-indigo-500";
                            "Bypass validation"
                        }
                    }
                }
                div #execute-result {}
                script {
                    (PreEscaped(format!(
                        r#"function umariExec_{fn_name}(btn) {{
  const form = btn.closest('form');
  const bypass = form.querySelector('[data-bypass-validation]')?.checked;
  if (!bypass && !form.reportValidity()) return;
  const obj = {{}};
  form.querySelectorAll('[data-field]').forEach(el => {{
    const key = el.dataset.field;
    const type = el.dataset.type;
    if (type === 'boolean') {{ obj[key] = el.checked; return; }}
    if (el.value === '') return;
    obj[key] = (type === 'integer') ? parseInt(el.value, 10)
             : (type === 'number')  ? parseFloat(el.value)
             : el.value;
  }});
  htmx.ajax('POST', '{execute_url}', {{
    target: '#execute-result',
    swap: 'innerHTML',
    values: {{ payload: JSON.stringify(obj) }}
  }});
}}"#
                    )))
                }
            }
        }
    } else {
        html! {
            section class="bg-white rounded-lg border border-gray-200 p-5 mt-6" {
                h3 class="text-base font-semibold text-gray-700 mb-3 mt-0" { "Execute Command" }
                form
                    hx-post={"/ui/commands/" (name) "/execute"}
                    hx-target="#execute-result"
                    hx-swap="innerHTML"
                    class="flex flex-col gap-4"
                {
                    label class="flex flex-col gap-1 text-sm font-medium text-gray-700" {
                        "JSON Payload"
                        textarea name="payload" rows="6" placeholder="{}"
                            class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
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
