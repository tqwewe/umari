use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, anyhow};
use colored::Colorize;
use serde::Deserialize;

use crate::client::ApiClient;

#[derive(Deserialize)]
struct CargoMetadata {
    workspace_root: PathBuf,
    target_directory: PathBuf,
    packages: Vec<CargoPackage>,
}

#[derive(Deserialize)]
struct CargoPackage {
    name: String,
    version: String,
    manifest_path: PathBuf,
    targets: Vec<CargoTarget>,
}

#[derive(Deserialize)]
struct CargoTarget {
    kind: Vec<String>,
}

#[derive(Deserialize)]
struct NpmPackage {
    name: String,
    version: String,
    umari: Option<UmariConfig>,
}

#[derive(Deserialize, Default)]
struct UmariConfig {
    wasm: Option<String>,
}

fn detect_module_type(path: &Path) -> Option<&'static str> {
    for component in path.components() {
        match component.as_os_str().to_str()? {
            "commands" => return Some("commands"),
            "projectors" => return Some("projectors"),
            "policies" => return Some("policies"),
            "effects" => return Some("effects"),
            _ => {}
        }
    }
    None
}

struct Module {
    name: String,
    version: String,
    module_type: &'static str,
    wasm_path: PathBuf,
}

struct JsModule {
    name: String,
    version: String,
    module_type: &'static str,
    dir: PathBuf,
    wasm_path: PathBuf,
}

enum AnyModule {
    Rust(Module),
    Js(JsModule),
}

impl AnyModule {
    fn name(&self) -> &str {
        match self {
            AnyModule::Rust(m) => &m.name,
            AnyModule::Js(m) => &m.name,
        }
    }

    fn version(&self) -> &str {
        match self {
            AnyModule::Rust(m) => &m.version,
            AnyModule::Js(m) => &m.version,
        }
    }

    fn module_type(&self) -> &'static str {
        match self {
            AnyModule::Rust(m) => m.module_type,
            AnyModule::Js(m) => m.module_type,
        }
    }

    fn wasm_path(&self) -> &Path {
        match self {
            AnyModule::Rust(m) => &m.wasm_path,
            AnyModule::Js(m) => &m.wasm_path,
        }
    }
}

fn discover_modules(filter_paths: &[PathBuf], debug: bool) -> Result<(Vec<AnyModule>, PathBuf)> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|err| anyhow!("failed to run cargo metadata: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("cargo metadata failed: {stderr}"));
    }

    let metadata: CargoMetadata = serde_json::from_slice(&output.stdout)
        .map_err(|err| anyhow!("failed to parse cargo metadata: {err}"))?;

    let profile = if debug { "debug" } else { "release" };

    let canonicalized_filters: Vec<PathBuf> = filter_paths
        .iter()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
        .collect();

    let mut matched_filters = vec![false; canonicalized_filters.len()];

    let mut modules = Vec::new();

    // Rust modules
    for pkg in &metadata.packages {
        let is_cdylib = pkg
            .targets
            .iter()
            .any(|t| t.kind.iter().any(|k| k == "cdylib"));
        if !is_cdylib {
            continue;
        }

        let Some(module_type) = detect_module_type(&pkg.manifest_path) else {
            continue;
        };

        if !canonicalized_filters.is_empty() {
            let pkg_dir = pkg.manifest_path.parent().unwrap_or(&pkg.manifest_path);
            let canonical_pkg_dir = pkg_dir
                .canonicalize()
                .unwrap_or_else(|_| pkg_dir.to_path_buf());

            let mut matches = false;
            for (i, filter) in canonicalized_filters.iter().enumerate() {
                if canonical_pkg_dir.starts_with(filter) {
                    matched_filters[i] = true;
                    matches = true;
                }
            }
            if !matches {
                continue;
            }
        }

        let wasm_name = pkg.name.replace('-', "_");
        let wasm_path = metadata
            .target_directory
            .join("wasm32-wasip2")
            .join(profile)
            .join(format!("{wasm_name}.wasm"));

        modules.push(AnyModule::Rust(Module {
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            module_type,
            wasm_path,
        }));
    }

    // JS modules
    for type_dir in &["commands", "projectors", "policies", "effects"] {
        let dir = metadata.workspace_root.join(type_dir);
        if !dir.exists() {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let module_dir = entry.path();
            if !module_dir.is_dir() {
                continue;
            }
            let pkg_path = module_dir.join("package.json");
            if !pkg_path.exists() {
                continue;
            }
            let content = match std::fs::read_to_string(&pkg_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let pkg: NpmPackage = match serde_json::from_str(&content) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let Some(umari_config) = pkg.umari else {
                continue;
            };
            let Some(module_type) = detect_module_type(&module_dir) else {
                continue;
            };

            if !canonicalized_filters.is_empty() {
                let canonical_dir = module_dir
                    .canonicalize()
                    .unwrap_or_else(|_| module_dir.clone());
                let mut matches = false;
                for (i, filter) in canonicalized_filters.iter().enumerate() {
                    if canonical_dir.starts_with(filter) {
                        matched_filters[i] = true;
                        matches = true;
                    }
                }
                if !matches {
                    continue;
                }
            }

            let wasm_rel = umari_config
                .wasm
                .unwrap_or_else(|| "dist/module.wasm".to_string());
            let wasm_path = module_dir.join(&wasm_rel);

            modules.push(AnyModule::Js(JsModule {
                name: pkg.name,
                version: pkg.version,
                module_type,
                dir: module_dir,
                wasm_path,
            }));
        }
    }

    for (i, filter) in filter_paths.iter().enumerate() {
        if !matched_filters[i] {
            eprintln!(
                "warning: no modules found matching path '{}'",
                filter.display()
            );
        }
    }

    Ok((modules, metadata.workspace_root))
}

pub fn build(paths: Vec<PathBuf>, debug: bool) -> Result<()> {
    let (modules, _workspace_root) = discover_modules(&paths, debug)?;

    if modules.is_empty() {
        return Err(anyhow!("no wasm modules found"));
    }

    println!("building {} module(s)...", modules.len());

    for module in &modules {
        match module {
            AnyModule::Rust(m) => {
                let mut args = vec!["build", "-p", &m.name, "--target", "wasm32-wasip2"];
                if !debug {
                    args.push("--release");
                }
                let status = Command::new("cargo")
                    .args(&args)
                    .status()
                    .map_err(|err| anyhow!("failed to run cargo build: {err}"))?;
                if !status.success() {
                    return Err(anyhow!("build failed for {}", m.name));
                }
                println!("{} built {} v{}", "✓".green(), m.name, m.version);
            }
            AnyModule::Js(m) => {
                let status = Command::new("npm")
                    .args(["run", "build"])
                    .current_dir(&m.dir)
                    .status()
                    .map_err(|err| anyhow!("failed to run npm: {err}"))?;
                if !status.success() {
                    return Err(anyhow!("build failed for {}", m.name));
                }
                println!("{} built {} v{}", "✓".green(), m.name, m.version);
            }
        }
    }

    println!("{} module(s) built", modules.len());
    Ok(())
}

pub fn deploy(
    client: &ApiClient,
    paths: Vec<PathBuf>,
    no_activate: bool,
    debug: bool,
) -> Result<()> {
    let (modules, _workspace_root) = discover_modules(&paths, debug)?;

    if modules.is_empty() {
        return Err(anyhow!("no wasm modules found"));
    }

    println!("building {} module(s)...", modules.len());

    for module in &modules {
        match module {
            AnyModule::Rust(m) => {
                let mut args = vec!["build", "-p", &m.name, "--target", "wasm32-wasip2"];
                if !debug {
                    args.push("--release");
                }
                let status = Command::new("cargo")
                    .args(&args)
                    .status()
                    .map_err(|err| anyhow!("failed to run cargo build: {err}"))?;
                if !status.success() {
                    return Err(anyhow!("build failed for {}", m.name));
                }
                println!("{} built {} v{}", "✓".green(), m.name, m.version);
            }
            AnyModule::Js(m) => {
                let status = Command::new("npm")
                    .args(["run", "build"])
                    .current_dir(&m.dir)
                    .status()
                    .map_err(|err| anyhow!("failed to run npm: {err}"))?;
                if !status.success() {
                    return Err(anyhow!("build failed for {}", m.name));
                }
                println!("{} built {} v{}", "✓".green(), m.name, m.version);
            }
        }
    }

    println!("uploading {} module(s)...", modules.len());

    for module in &modules {
        let wasm_path = module.wasm_path();
        if !wasm_path.exists() {
            return Err(anyhow!(
                "wasm file not found at '{}' for module '{}'",
                wasm_path.display(),
                module.name()
            ));
        }

        client.upload_wasm(
            module.module_type(),
            module.name(),
            module.version(),
            wasm_path,
            !no_activate,
        )?;

        println!(
            "{} deployed {} v{} ({})",
            "✓".green(),
            module.name(),
            module.version(),
            module.module_type()
        );
    }

    println!("{} module(s) deployed", modules.len());
    Ok(())
}
