use std::{collections::HashMap, sync::Arc};

use maud::{Markup, html};
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
                        th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "Actions" }
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
                            td class="px-4 py-3" {
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

    html! {
        section class="bg-white rounded-lg border border-gray-200 p-5 mt-6" {
            h3 class="text-base font-semibold text-gray-700 mb-3 mt-0" { "Upload New Version" }
            form
                hx-post={"/ui/upload/" (module_type_str)}
                hx-encoding="multipart/form-data"
                class="flex flex-col gap-4"
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
                button type="submit"
                    class="self-start inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
                    { "Upload" }
            }
        }
    }
}

pub fn execute_form(name: &str) -> Markup {
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
