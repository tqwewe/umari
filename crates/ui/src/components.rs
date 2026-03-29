use std::{collections::HashMap, sync::Arc};

use maud::{Markup, PreEscaped, html};
use schemars::Schema;
use semver::Version;
use umari_runtime::module_store::{Module, ModuleType, ModuleVersionInfo};

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

pub fn versions_table(
    module_type: ModuleType,
    name: &str,
    versions: &[ModuleVersionInfo],
    active_version: Option<&Version>,
) -> Markup {
    let module_type_str = match module_type {
        ModuleType::Command => "commands",
        ModuleType::Policy => "policies",
        ModuleType::Projector => "projectors",
        ModuleType::Effect => "effects",
    };
    let table_id = format!("versions-table-{name}");

    html! {
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
                    @for info in versions {
                        @let is_active = active_version.is_some_and(|v| v == &info.version);
                        @let sha_short = &info.sha256[..12.min(info.sha256.len())];
                        tr class="border-b border-gray-100 last:border-0 hover:bg-gray-50" {
                            td class="px-4 py-3 text-gray-700 font-mono text-xs" { (info.version) }
                            td class="px-4 py-3" {
                                @if is_active {
                                    span class="text-emerald-500 font-semibold" { "✓" }
                                }
                            }
                            td class="px-4 py-3 text-gray-500 font-mono text-xs" {
                                span title=(info.sha256) { (sha_short) "…" }
                            }
                            td class="px-4 py-3 text-right" {
                                @if is_active {
                                    button
                                        hx-delete={"/ui/" (module_type_str) "/" (name) "/active"}
                                        hx-target={"#" (table_id)}
                                        hx-swap="outerHTML"
                                        class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-white border border-gray-300 text-gray-700 hover:bg-gray-50 transition-colors"
                                        { "Deactivate" }
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

        let input_type = if type_str == "string" && enum_vals.is_some() {
            let values: Vec<String> = enum_vals
                .unwrap()
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
        });
    }

    Some(fields)
}

pub fn execute_form(name: &str, schema: Option<&Schema>) -> Markup {
    let fields = schema.and_then(parse_fields);

    if let Some(fields) = fields {
        let form_id = format!("exec-{name}");
        let execute_url = format!("/ui/commands/{name}/execute");
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
                                        data-field=(field.key)
                                        data-type="string"
                                        placeholder=[field.placeholder]
                                        required[field.required]
                                        class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                }
                                InputType::Email => {
                                    input type="email"
                                        data-field=(field.key)
                                        data-type="string"
                                        placeholder="user@example.com"
                                        required[field.required]
                                        class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                }
                                InputType::Date => {
                                    input type="date"
                                        data-field=(field.key)
                                        data-type="string"
                                        required[field.required]
                                        class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                }
                                InputType::DateTime => {
                                    input type="datetime-local"
                                        data-field=(field.key)
                                        data-type="string"
                                        required[field.required]
                                        class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";
                                }
                                InputType::Number { integer } => {
                                    input type="number"
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
                                        data-field=(field.key)
                                        data-type="boolean"
                                        class="h-4 w-4 rounded border-gray-300 text-indigo-600 focus:ring-indigo-500";
                                }
                                InputType::Select(options) => {
                                    select data-field=(field.key)
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
                    button type="button"
                        onclick={
                            "umariExec_" (name) "(this)"
                        }
                        class="self-start inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
                        { "Execute" }
                }
                div #execute-result {}
                script {
                    (PreEscaped(format!(
                        r#"function umariExec_{name}(btn) {{
  const form = btn.closest('form');
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
