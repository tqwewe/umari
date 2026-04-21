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
        "effect" => "effects",
        _ => unreachable!(),
    }
}

fn cargo_toml_content(module_type: &str, name: &str) -> String {
    match module_type {
        "command" => format!(
            "[package]\nname = \"{name}\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\numari.workspace = true\nserde.workspace = true\nwit-bindgen.workspace = true\n"
        ),
        _ => format!(
            "[package]\nname = \"{name}\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\numari.workspace = true\n"
        ),
    }
}

fn lib_rs_content(module_type: &str, type_name: &str) -> String {
    match module_type {
        "command" => format!(
            "use serde::Deserialize;\nuse umari::prelude::*;\n\nexport_command!({type_name});\n\n#[derive(EventSet)]\nenum Query {{\n    // TODO: add event variants, e.g.: MyEvent(MyEvent),\n}}\n\n#[derive(CommandInput, Deserialize)]\nstruct Input {{\n    // TODO: add input fields; use #[domain_id] to tag domain ID fields\n}}\n\n#[derive(Default)]\nstruct {type_name} {{}}\n\nimpl Command for {type_name} {{\n    type Query = Query;\n    type Input = Input;\n\n    fn apply(&mut self, event: Query, _meta: EventMeta) {{\n        match event {{}}\n    }}\n\n    fn handle(&self, input: Input) -> Result<Emit, CommandError> {{\n        Ok(emit![])\n    }}\n}}\n"
        ),
        "projector" => format!(
            "use umari::prelude::*;\n\nexport_projector!({type_name});\n\n#[derive(EventSet)]\nenum Query {{\n    // TODO: add event variants, e.g.: MyEvent(MyEvent),\n}}\n\nstruct {type_name} {{}}\n\nimpl Projector for {type_name} {{\n    type Query = Query;\n\n    fn init() -> Result<Self, ProjectorError> {{\n        // TODO: run CREATE TABLE IF NOT EXISTS statements here\n        Ok({type_name} {{}})\n    }}\n\n    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), ProjectorError> {{\n        match event.data {{}}\n    }}\n}}\n"
        ),
        "effect" => format!(
            "use umari::prelude::*;\n\nexport_effect!({type_name});\n\n#[derive(EventSet)]\nenum Query {{\n    // TODO: add event variants, e.g.: MyEvent(MyEvent),\n}}\n\n#[derive(Default)]\nstruct {type_name} {{}}\n\nimpl Effect for {type_name} {{\n    type Query = Query;\n    type Error = String;\n\n    fn partition_key(&self, _event: StoredEvent<Self::Query>) -> Option<String> {{\n        None\n    }}\n\n    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), CommandError> {{\n        Ok(())\n    }}\n}}\n"
        ),
        _ => unreachable!(),
    }
}

fn package_json_content(module_type: &str, name: &str) -> String {
    let extra_deps = if module_type == "command" {
        ",\n    \"zod\": \"^3.0.0\""
    } else {
        ""
    };
    format!(
        "{{\n  \"name\": \"{name}\",\n  \"version\": \"0.1.0\",\n  \"umari\": {{\n    \"wasm\": \"dist/module.wasm\"\n  }},\n  \"scripts\": {{\n    \"build\": \"esbuild src/index.ts --bundle --outfile=dist/bundle.js --format=esm --platform=neutral --external:'umari:*' && jco componentize dist/bundle.js --wit node_modules/@umari/js/wit/{module_type} --world-name {module_type} --out dist/module.wasm\"\n  }},\n  \"devDependencies\": {{\n    \"@bytecodealliance/jco\": \"^1.17.6\",\n    \"@umari/js\": \"../../packages/js\",\n    \"esbuild\": \"^0.25.0\"{extra_deps}\n  }}\n}}\n"
    )
}

fn tsconfig_json_content() -> &'static str {
    "{\n  \"extends\": \"../../packages/js/tsconfig.json\",\n  \"include\": [\"src\"]\n}\n"
}

fn index_ts_content(module_type: &str, type_name: &str) -> String {
    match module_type {
        "command" => format!(
            "import {{ z }} from \"zod\";\nimport {{ defineCommand, exportCommand }} from \"@umari/js\";\n\nconst {type_name} = defineCommand({{\n  input: z.object({{\n    // TODO: add fields; domain ID fields must be in domainIds below\n  }}),\n  domainIds: [],\n  emit(_state, _input) {{\n    return [];\n  }},\n}});\n\nexport const {{ schema, query, execute }} = exportCommand({type_name});\n"
        ),
        "projector" => format!(
            "import {{ exportProjector }} from \"@umari/js\";\nimport type {{ SqliteDb }} from \"@umari/js\";\n\nexport const {type_name} = exportProjector({{\n  events: [],\n  setup(_db: SqliteDb) {{\n    // TODO: CREATE TABLE IF NOT EXISTS ...\n  }},\n  handle(_event, _db) {{}},\n}});\n"
        ),
        "effect" => format!(
            "import {{ exportEffect }} from \"@umari/js\";\n\nexport const {type_name} = exportEffect({{\n  events: [],\n  handle(_event) {{}},\n}});\n"
        ),
        _ => unreachable!(),
    }
}

pub fn generate_js(module_type: &str, name: &str) -> Result<()> {
    let root = workspace_root()?;
    let plural = type_plural(module_type);
    let module_dir = std::path::Path::new(&root).join(plural).join(name);

    if module_dir.exists() {
        bail!("directory already exists: {}", module_dir.display());
    }

    let src_dir = module_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let type_name = kebab_to_pascal(name);

    fs::write(
        module_dir.join("package.json"),
        package_json_content(module_type, name),
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
    println!("  cd {plural}/{name} && npm install");

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

    fs::write(
        crate_dir.join("Cargo.toml"),
        cargo_toml_content(module_type, name),
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
