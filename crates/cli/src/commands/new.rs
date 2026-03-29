use std::fs;
use std::process::Command;

use anyhow::{Result, anyhow, bail};
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

fn type_plural(module_type: &str) -> &str {
    match module_type {
        "command" => "commands",
        "projector" => "projectors",
        "policy" => "policies",
        "effect" => "effects",
        _ => unreachable!(),
    }
}

fn cargo_toml_content(module_type: &str, name: &str) -> String {
    match module_type {
        "command" => format!(
            "[package]\nname = \"{name}\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\numari-core.workspace = true\nserde.workspace = true\nwit-bindgen.workspace = true\n"
        ),
        _ => format!(
            "[package]\nname = \"{name}\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\numari-core.workspace = true\n"
        ),
    }
}

fn lib_rs_content(module_type: &str, type_name: &str) -> String {
    match module_type {
        "command" => format!(
            "use serde::Deserialize;\nuse umari_core::prelude::*;\n\nexport_command!({type_name});\n\n#[derive(EventSet)]\nenum Query {{\n    // TODO: add event variants, e.g.: MyEvent(MyEvent),\n}}\n\n#[derive(CommandInput, Deserialize)]\nstruct Input {{\n    // TODO: add input fields; use #[domain_id] to tag domain ID fields\n}}\n\n#[derive(Default)]\nstruct {type_name} {{}}\n\nimpl Command for {type_name} {{\n    type Query = Query;\n    type Input = Input;\n    type Error = CommandError;\n\n    fn apply(&mut self, event: Query, _meta: EventMeta) {{\n        match event {{}}\n    }}\n\n    fn handle(&self, input: Input) -> Result<Emit, CommandError> {{\n        Ok(emit![])\n    }}\n}}\n"
        ),
        "projector" => format!(
            "use umari_core::prelude::*;\n\nexport_projector!({type_name});\n\n#[derive(EventSet)]\nenum Query {{\n    // TODO: add event variants, e.g.: MyEvent(MyEvent),\n}}\n\nstruct {type_name} {{}}\n\nimpl Projector for {type_name} {{\n    type Query = Query;\n\n    fn init() -> Result<Self, ProjectorError> {{\n        // TODO: run CREATE TABLE IF NOT EXISTS statements here\n        Ok({type_name} {{}})\n    }}\n\n    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), ProjectorError> {{\n        match event.data {{}}\n    }}\n}}\n"
        ),
        "policy" => format!(
            "use umari_core::prelude::*;\n\nexport_policy!({type_name});\n\n#[derive(EventSet)]\nenum Query {{\n    // TODO: add event variants, e.g.: MyEvent(MyEvent),\n}}\n\n#[derive(Default)]\nstruct {type_name} {{}}\n\nimpl Policy for {type_name} {{\n    type Query = Query;\n\n    fn partition_key(&self, event: StoredEvent<Self::Query>) -> Option<String> {{\n        None\n    }}\n\n    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<Vec<CommandSubmission>, SqliteError> {{\n        Ok(vec![])\n    }}\n}}\n"
        ),
        "effect" => format!(
            "use umari_core::prelude::*;\n\nexport_effect!({type_name});\n\n#[derive(EventSet)]\nenum Query {{\n    // TODO: add event variants, e.g.: MyEvent(MyEvent),\n}}\n\n#[derive(Default)]\nstruct {type_name} {{}}\n\nimpl Effect for {type_name} {{\n    type Query = Query;\n    type Error = String;\n\n    fn partition_key(&self, _event: StoredEvent<Self::Query>) -> Option<String> {{\n        None\n    }}\n\n    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), Self::Error> {{\n        Ok(())\n    }}\n}}\n"
        ),
        _ => unreachable!(),
    }
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

    fs::write(crate_dir.join("Cargo.toml"), cargo_toml_content(module_type, name))?;
    fs::write(src_dir.join("lib.rs"), lib_rs_content(module_type, &type_name))?;

    // register in workspace Cargo.toml
    let workspace_toml_path = std::path::Path::new(&root).join("Cargo.toml");
    let content = fs::read_to_string(&workspace_toml_path)?;
    let mut doc = content.parse::<toml_edit::DocumentMut>()
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
