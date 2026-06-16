use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, bail};
use indoc::formatdoc;

use super::new::umari_js_dep;

const RUST_TOOLCHAIN: &str = "[toolchain]\ntargets = [\"wasm32-wasip2\"]\n";

const SHARED_TSCONFIG: &str = "{\n  \"extends\": \"../tsconfig.json\",\n  \"include\": [\"src\"]\n}\n";

const SHARED_INDEX_TS: &str = r#"// Shared event & fold definitions. Import these from your modules, e.g.:
//
//   import { defineEvent } from "@umari/js";
//
//   export const UserRegistered = defineEvent<{ userId: bigint }>()("user.registered", {
//     domainIds: ["userId"],
//   });

export {};
"#;

const JS_TSCONFIG: &str = r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "lib": ["ES2022"],
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "noFallthroughCasesInSwitch": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "isolatedModules": true,
    "resolveJsonModule": true,
    "types": ["node"]
  }
}
"#;

fn target_dir(path: Option<&str>) -> Result<PathBuf> {
    match path {
        Some(p) => Ok(PathBuf::from(p)),
        None => Ok(std::env::current_dir()?),
    }
}

fn project_name(dir: &Path) -> Result<String> {
    let abs = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        std::env::current_dir()?.join(dir)
    };
    let name = abs
        .file_name()
        .and_then(|n| n.to_str())
        .map(sanitize_name)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "umari-project".to_string());
    Ok(name)
}

fn sanitize_name(raw: &str) -> String {
    let mapped: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    mapped.trim_matches('-').to_string()
}

fn ensure_no_manifest(dir: &Path, manifest: &str) -> Result<()> {
    let path = dir.join(manifest);
    if path.exists() {
        bail!("{} already exists; refusing to overwrite", path.display());
    }
    Ok(())
}

/// Write a file only if it does not already exist, so init is non-destructive.
fn write_new(path: &Path, content: &str) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    fs::write(path, content)?;
    Ok(true)
}

fn in_git_repo(dir: &Path) -> bool {
    dir.ancestors().any(|d| d.join(".git").exists())
}

fn maybe_git_init(dir: &Path) {
    if in_git_repo(dir) {
        return;
    }
    let _ = Command::new("git").arg("init").arg(dir).output();
}

fn rust_cargo_toml(name: &str) -> String {
    formatdoc! {r#"
        [package]
        name = "{name}"
        version = "0.1.0"
        edition = "2024"

        [dependencies]
        anyhow.workspace = true
        schemars.workspace = true
        serde.workspace = true
        umari.workspace = true

        [workspace]
        resolver = "2"
        members = ["."]

        [workspace.dependencies]
        {name} = {{ path = "." }}
        umari = "0.2"
        anyhow = "1.0"
        schemars = "1.2"
        serde = {{ version = "1.0", features = ["derive"] }}
        serde_json = "1.0"
        uuid = {{ version = "1.22", features = ["serde", "v4", "v5"] }}
        validator = {{ version = "0.20", features = ["derive"] }}
        wasi-http-client = {{ version = "0.2", features = ["json"] }}
    "#}
}

fn shared_package_json(name: &str) -> String {
    let umari_js = umari_js_dep();
    formatdoc! {r#"
        {{
          "name": "@{name}/shared",
          "version": "0.1.0",
          "private": true,
          "type": "module",
          "main": "./src/index.ts",
          "types": "./src/index.ts",
          "exports": {{
            ".": {{
              "types": "./src/index.ts",
              "default": "./src/index.ts"
            }}
          }},
          "devDependencies": {{
            "@umari/js": "{umari_js}"
          }}
        }}
    "#}
}

fn js_package_json(name: &str) -> String {
    let umari_js = umari_js_dep();
    formatdoc! {r#"
        {{
          "name": "{name}",
          "version": "0.1.0",
          "private": true,
          "type": "module",
          "workspaces": [
            "shared",
            "commands/*",
            "projectors/*",
            "effects/*"
          ],
          "devDependencies": {{
            "@umari/js": "{umari_js}",
            "@bytecodealliance/jco": "^1.24.1",
            "@types/node": "^25.9.3",
            "esbuild": "^0.28.1",
            "typescript": "^6.0.3"
          }}
        }}
    "#}
}

pub fn init_rust(path: Option<&str>) -> Result<()> {
    let dir = target_dir(path)?;
    fs::create_dir_all(&dir)?;
    ensure_no_manifest(&dir, "Cargo.toml")?;

    let name = project_name(&dir)?;
    let src = dir.join("src");
    fs::create_dir_all(src.join("events"))?;
    fs::create_dir_all(src.join("folds"))?;

    write_new(&dir.join("Cargo.toml"), &rust_cargo_toml(&name))?;
    write_new(&src.join("lib.rs"), "pub mod events;\npub mod folds;\n")?;
    write_new(&src.join("events").join("mod.rs"), "")?;
    write_new(&src.join("folds").join("mod.rs"), "")?;
    write_new(&dir.join("rust-toolchain.toml"), RUST_TOOLCHAIN)?;
    write_new(&dir.join(".gitignore"), "/target\n")?;

    maybe_git_init(&dir);
    print_next_steps(path, &name, false);
    Ok(())
}

pub fn init_js(path: Option<&str>) -> Result<()> {
    let dir = target_dir(path)?;
    fs::create_dir_all(&dir)?;
    ensure_no_manifest(&dir, "package.json")?;

    let name = project_name(&dir)?;
    write_new(&dir.join("package.json"), &js_package_json(&name))?;
    write_new(&dir.join("tsconfig.json"), JS_TSCONFIG)?;
    write_new(&dir.join(".gitignore"), "node_modules\ndist\n")?;

    let shared = dir.join("shared");
    fs::create_dir_all(shared.join("src"))?;
    write_new(&shared.join("package.json"), &shared_package_json(&name))?;
    write_new(&shared.join("tsconfig.json"), SHARED_TSCONFIG)?;
    write_new(&shared.join("src").join("index.ts"), SHARED_INDEX_TS)?;

    maybe_git_init(&dir);
    print_next_steps(path, &name, true);
    Ok(())
}

fn print_next_steps(path: Option<&str>, name: &str, js: bool) {
    println!("created umari workspace '{name}'");
    println!();
    println!("next steps:");
    if let Some(p) = path {
        if p != "." {
            println!("  cd {p}");
        }
    }
    if js {
        println!("  npm install");
    }
    println!("  umari new command <name>");
}
