use maud::{Markup, html};
use semver::Version;
use umari_runtime::module_store::{ModuleType, ModuleVersionInfo};

pub fn versions_table(
    module_type: ModuleType,
    name: &str,
    versions: &[ModuleVersionInfo],
    active_version: Option<&Version>,
) -> Markup {
    let module_type_str = match module_type {
        ModuleType::Command => "commands",
        ModuleType::Policy => "policies",
        ModuleType::Projection => "projections",
        ModuleType::Effect => "effects",
    };
    let table_id = format!("versions-table-{name}");

    html! {
        table id=(table_id) {
            thead {
                tr {
                    th { "Version" }
                    th { "Active" }
                    th { "SHA256" }
                    th { "Actions" }
                }
            }
            tbody {
                @if versions.is_empty() {
                    tr {
                        td colspan="4" { "No versions uploaded yet." }
                    }
                }
                @for info in versions {
                    @let is_active = active_version.is_some_and(|v| v == &info.version);
                    @let sha_short = &info.sha256[..12.min(info.sha256.len())];
                    tr {
                        td { (info.version) }
                        td {
                            @if is_active {
                                span { "✓" }
                            }
                        }
                        td {
                            span title=(info.sha256) { (sha_short) "…" }
                        }
                        td {
                            @if is_active {
                                button
                                    hx-delete={"/ui/" (module_type_str) "/" (name) "/active"}
                                    hx-target={"#" (table_id)}
                                    hx-swap="outerHTML"
                                    class="secondary"
                                    { "Deactivate" }
                            } @else {
                                button
                                    hx-put={"/ui/" (module_type_str) "/" (name) "/active"}
                                    hx-vals={"{\"version\":\"" (info.version) "\"}"}
                                    hx-target={"#" (table_id)}
                                    hx-swap="outerHTML"
                                    { "Activate" }
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
        ModuleType::Projection => "projections",
        ModuleType::Effect => "effects",
    };

    html! {
        section {
            h3 { "Upload New Version" }
            form
                hx-post={"/ui/upload/" (module_type_str)}
                hx-encoding="multipart/form-data"
            {
                @if let Some(n) = name {
                    input type="hidden" name="name" value=(n);
                } @else {
                    label {
                        "Name"
                        input type="text" name="name" required placeholder="module-name";
                    }
                }
                label {
                    "Version"
                    input type="text" name="version" required placeholder="1.0.0";
                }
                label {
                    "WASM file"
                    input type="file" name="wasm" accept=".wasm" required;
                }
                label {
                    input type="checkbox" name="activate" value="true";
                    " Activate immediately"
                }
                button type="submit" { "Upload" }
            }
        }
    }
}

pub fn execute_form(name: &str) -> Markup {
    html! {
        section {
            h3 { "Execute Command" }
            form
                hx-post={"/ui/commands/" (name) "/execute"}
                hx-target="#execute-result"
                hx-swap="innerHTML"
            {
                label {
                    "JSON Payload"
                    textarea name="payload" rows="6" placeholder="{}" {}
                }
                button type="submit" { "Execute" }
            }
            div #execute-result {}
        }
    }
}
