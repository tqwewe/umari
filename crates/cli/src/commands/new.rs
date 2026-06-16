use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, anyhow, bail};
use indoc::formatdoc;
use serde::Deserialize;

#[derive(Deserialize)]
struct CargoMetadata {
    workspace_root: String,
}

fn workspace_root() -> Result<String> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()?;
    if !output.status.success() {
        bail!("cargo metadata failed");
    }
    let meta: CargoMetadata = serde_json::from_slice(&output.stdout)?;
    Ok(meta.workspace_root)
}

fn has_workspace_marker(dir: &Path) -> bool {
    if dir.join(".git").exists() {
        return true;
    }
    if let Ok(content) = fs::read_to_string(dir.join("Cargo.toml")) {
        if content.contains("[workspace]") {
            return true;
        }
    }
    if let Ok(content) = fs::read_to_string(dir.join("package.json")) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if json.get("workspaces").is_some() {
                return true;
            }
        }
    }
    false
}

fn js_workspace_root() -> Result<String> {
    let cwd = std::env::current_dir()?;
    let mut current = cwd.as_path();
    loop {
        if has_workspace_marker(current) {
            return Ok(current.to_string_lossy().into_owned());
        }
        match current.parent() {
            Some(p) => current = p,
            None => return Ok(cwd.to_string_lossy().into_owned()),
        }
    }
}

/// The `@umari/js` dependency spec written into generated package.json files.
/// Defaults to the published version; override with UMARI_JS_DEP for local
/// development against an unpublished SDK (e.g. a `file:` path).
pub fn umari_js_dep() -> String {
    std::env::var("UMARI_JS_DEP").unwrap_or_else(|_| "^0.1.0".to_string())
}

fn kebab_to_pascal(name: &str) -> String {
    name.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

fn workspace_package_name(root: &str) -> Option<String> {
    let content = fs::read_to_string(std::path::Path::new(root).join("Cargo.toml")).ok()?;
    let doc = content.parse::<toml_edit::DocumentMut>().ok()?;
    doc.get("package")?
        .get("name")?
        .as_str()
        .map(|s| s.to_string())
}

fn type_plural(module_type: &str) -> &str {
    match module_type {
        "command" => "commands",
        "projector" => "projectors",
        "effect" => "effects",
        _ => unreachable!(),
    }
}

fn cargo_toml_content(module_type: &str, name: &str, workspace_pkg: Option<&str>) -> String {
    let extra_dep = workspace_pkg
        .map(|pkg| format!("{pkg}.workspace = true\n\n"))
        .unwrap_or_default();
    let serde_dep = if module_type == "command" {
        "serde.workspace = true\n"
    } else {
        ""
    };
    formatdoc! {r#"
        [package]
        name = "{name}"
        version = "0.1.0"
        edition = "2024"

        [lib]
        crate-type = ["cdylib", "rlib"]

        [dependencies]
        {extra_dep}anyhow.workspace = true
        schemars.workspace = true
        {serde_dep}umari.workspace = true
    "#}
}

fn lib_rs_content(module_type: &str, type_name: &str) -> String {
    match module_type {
        "command" => {
            let _ = type_name;
            formatdoc! {r#"
                use schemars::JsonSchema;
                use serde::Deserialize;
                use umari::prelude::*;

                #[derive(DomainIds, JsonSchema, Deserialize)]
                pub struct Input {{
                    // TODO: add input fields; use #[domain_id] to tag domain ID fields
                }}

                #[export_command]
                pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {{
                    Command::new(input, context).execute(|input| {{
                        // TODO: implement execute
                        Ok(emit![])
                    }})
                }}
            "#}
        }
        "projector" => formatdoc! {r#"
            use umari::prelude::*;

            export_projector!({type_name});

            #[derive(EventSet)]
            enum Query {{
                // TODO: add event variants, e.g.: MyEvent(MyEvent),
            }}

            struct {type_name} {{}}

            impl Projector for {type_name} {{
                type Query = Query;

                fn init() -> anyhow::Result<Self> {{
                    // TODO: run CREATE TABLE IF NOT EXISTS statements here
                    Ok({type_name} {{}})
                }}

                fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {{
                    match event.data {{}}
                }}
            }}
        "#},
        "effect" => formatdoc! {r#"
            use umari::prelude::*;

            export_effect!({type_name});

            #[derive(EventSet)]
            enum Query {{
                // TODO: add event variants, e.g.: MyEvent(MyEvent),
            }}

            struct {type_name} {{}}

            impl Effect for {type_name} {{
                type Query = Query;

                fn init() -> anyhow::Result<Self> {{
                    Ok({type_name} {{}})
                }}

                fn partition_key(&self, _event: StoredEvent<Query>) -> Option<String> {{
                    None
                }}

                fn handle(&mut self, event: StoredEvent<Query>) -> anyhow::Result<()> {{
                    Ok(())
                }}
            }}
        "#},
        _ => unreachable!(),
    }
}

fn shared_package_name(root: &str) -> Option<String> {
    let content = fs::read_to_string(Path::new(root).join("shared").join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("name")?.as_str().map(|s| s.to_string())
}

fn package_json_content(module_type: &str, name: &str, shared_pkg: Option<&str>) -> String {
    let umari_js = umari_js_dep();
    // Build tooling (jco, esbuild, typescript) is hoisted from the workspace
    // root created by `umari init`; modules only declare what they import.
    let shared_dep = shared_pkg
        .map(|pkg| format!(",\n    \"{pkg}\": \"*\""))
        .unwrap_or_default();
    let extra_deps = if module_type == "command" {
        ",\n    \"zod\": \"^3.0.0\""
    } else {
        ""
    };
    formatdoc! {r#"
        {{
          "name": "{name}",
          "version": "0.1.0",
          "type": "module",
          "umari": {{
            "wasm": "dist/module.wasm"
          }},
          "scripts": {{
            "build": "umari-js build src/index.ts --out dist/module.wasm"
          }},
          "devDependencies": {{
            "@umari/js": "{umari_js}"{shared_dep}{extra_deps}
          }}
        }}
    "#}
}

fn tsconfig_json_content() -> &'static str {
    "{\n  \"extends\": \"../../tsconfig.json\",\n  \"include\": [\"src\"]\n}\n"
}

fn index_ts_content(module_type: &str, type_name: &str) -> String {
    match module_type {
        "command" => formatdoc! {r#"
            import {{ z }} from "zod";
            import {{ defineCommand, exportCommand, emit }} from "@umari/js";

            type Input = z.infer<typeof InputSchema>;

            const InputSchema = z.object({{
              // TODO: add fields. Domain ID fields must also appear in `domainIds` below.
            }});

            const {type_name} = defineCommand<Input, {{}}>({{
              input: InputSchema,
              domainIds: [] as const,
              folds: (_input) => ({{
                // TODO: declare bound folds, e.g. exists: UserExistsFold({{ userId: input.userId }})
              }}),
              execute: ({{ input: _input, folds: _folds }}) => {{
                return emit();
              }},
            }});

            export const {{ schema, execute }} = exportCommand({type_name});
        "#},
        "projector" => formatdoc! {r#"
            import {{ defineProjector, exportProjector, sqlite }} from "@umari/js";

            const {type_name} = defineProjector({{
              events: [
                // TODO: list event definitions this projector subscribes to.
              ],
              init: () => {{
                sqlite.executeBatch(`
                  -- TODO: CREATE TABLE IF NOT EXISTS …
                `);
              }},
              handle: (event) => {{
                switch (event.type) {{
                  // TODO: dispatch per event.type
                }}
              }},
            }});

            export const {{ projector }} = exportProjector({type_name});
        "#},
        "effect" => formatdoc! {r#"
            import {{ defineEffect, exportEffect }} from "@umari/js";

            const {type_name} = defineEffect({{
              events: [
                // TODO: list event definitions this effect subscribes to.
              ],
              init: () => {{
                return {{}};
              }},
              partitionKey: (_event) => undefined,
              handle: async (_event, _state) => {{
                // TODO: perform side effects, optionally call other commands via `execute(...)`.
              }},
            }});

            export const {{ effect }} = exportEffect({type_name});
        "#},
        _ => unreachable!(),
    }
}

pub fn generate_js(module_type: &str, name: &str) -> Result<()> {
    let root = js_workspace_root()?;
    let plural = type_plural(module_type);
    let module_dir = std::path::Path::new(&root).join(plural).join(name);

    if module_dir.exists() {
        bail!("directory already exists: {}", module_dir.display());
    }

    let src_dir = module_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let type_name = kebab_to_pascal(name);
    let shared_pkg = shared_package_name(&root);

    fs::write(
        module_dir.join("package.json"),
        package_json_content(module_type, name, shared_pkg.as_deref()),
    )?;
    fs::write(module_dir.join("tsconfig.json"), tsconfig_json_content())?;
    fs::write(
        src_dir.join("index.ts"),
        index_ts_content(module_type, &type_name),
    )?;

    println!("created {plural}/{name}");
    println!("  {plural}/{name}/package.json");
    println!("  {plural}/{name}/tsconfig.json");
    println!("  {plural}/{name}/src/index.ts");
    println!();
    println!("next steps:");
    println!("  npm install        # from the workspace root, to link the new module");

    Ok(())
}

pub fn generate(module_type: &str, name: &str) -> Result<()> {
    let root = workspace_root()?;
    let plural = type_plural(module_type);
    let crate_dir = std::path::Path::new(&root).join(plural).join(name);

    if crate_dir.exists() {
        bail!("directory already exists: {}", crate_dir.display());
    }

    let src_dir = crate_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let type_name = kebab_to_pascal(name);
    let workspace_pkg = workspace_package_name(&root);

    fs::write(
        crate_dir.join("Cargo.toml"),
        cargo_toml_content(module_type, name, workspace_pkg.as_deref()),
    )?;
    fs::write(
        src_dir.join("lib.rs"),
        lib_rs_content(module_type, &type_name),
    )?;

    // register in workspace Cargo.toml
    let workspace_toml_path = std::path::Path::new(&root).join("Cargo.toml");
    let content = fs::read_to_string(&workspace_toml_path)?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| anyhow!("failed to parse workspace Cargo.toml: {err}"))?;

    let members = doc["workspace"]["members"]
        .as_array_mut()
        .ok_or_else(|| anyhow!("workspace members is not an array"))?;

    let member_path = format!("{plural}/{name}");
    members.push(member_path);

    fs::write(&workspace_toml_path, doc.to_string())?;

    println!("created {plural}/{name}");
    println!("  {plural}/{name}/Cargo.toml");
    println!("  {plural}/{name}/src/lib.rs");
    println!();
    println!("next steps:");
    println!("  cargo check -p {name}");

    Ok(())
}
