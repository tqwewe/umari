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

fn detect_module_type(manifest_path: &Path) -> Option<&'static str> {
    for component in manifest_path.components() {
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

fn discover_modules(filter_paths: &[PathBuf], debug: bool) -> Result<(Vec<Module>, PathBuf)> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|err| anyhow!("failed to run cargo metadata: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("cargo metadata failed: {stderr}"));
    }

    let metadata: CargoMetadata =
        serde_json::from_slice(&output.stdout).map_err(|err| anyhow!("failed to parse cargo metadata: {err}"))?;

    let profile = if debug { "debug" } else { "release" };

    let canonicalized_filters: Vec<PathBuf> = filter_paths
        .iter()
        .map(|p| {
            p.canonicalize()
                .unwrap_or_else(|_| p.clone())
        })
        .collect();

    let mut matched_filters = vec![false; canonicalized_filters.len()];

    let mut modules = Vec::new();
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
            let canonical_pkg_dir = pkg_dir.canonicalize().unwrap_or_else(|_| pkg_dir.to_path_buf());

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

        modules.push(Module {
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            module_type,
            wasm_path,
        });
    }

    for (i, filter) in filter_paths.iter().enumerate() {
        if !matched_filters[i] {
            eprintln!("warning: no modules found matching path '{}'", filter.display());
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
        let mut args = vec!["build", "-p", &module.name, "--target", "wasm32-wasip2"];
        if !debug {
            args.push("--release");
        }

        let status = Command::new("cargo")
            .args(&args)
            .status()
            .map_err(|err| anyhow!("failed to run cargo build: {err}"))?;

        if !status.success() {
            return Err(anyhow!("build failed for {}", module.name));
        }

        println!("{} built {} v{}", "✓".green(), module.name, module.version);
    }

    println!("{} module(s) built", modules.len());
    Ok(())
}

pub fn deploy(client: &ApiClient, paths: Vec<PathBuf>, no_activate: bool, debug: bool) -> Result<()> {
    let (modules, _workspace_root) = discover_modules(&paths, debug)?;

    if modules.is_empty() {
        return Err(anyhow!("no wasm modules found"));
    }

    println!("building {} module(s)...", modules.len());

    for module in &modules {
        let mut args = vec!["build", "-p", &module.name, "--target", "wasm32-wasip2"];
        if !debug {
            args.push("--release");
        }

        let status = Command::new("cargo")
            .args(&args)
            .status()
            .map_err(|err| anyhow!("failed to run cargo build: {err}"))?;

        if !status.success() {
            return Err(anyhow!("build failed for {}", module.name));
        }

        println!("{} built {} v{}", "✓".green(), module.name, module.version);
    }

    println!("uploading {} module(s)...", modules.len());

    for module in &modules {
        if !module.wasm_path.exists() {
            return Err(anyhow!(
                "wasm file not found at '{}' for module '{}'",
                module.wasm_path.display(),
                module.name
            ));
        }

        client.upload_wasm(
            module.module_type,
            &module.name,
            &module.version,
            &module.wasm_path,
            !no_activate,
        )?;

        println!(
            "{} deployed {} v{} ({})",
            "✓".green(),
            module.name,
            module.version,
            module.module_type
        );
    }

    println!("{} module(s) deployed", modules.len());
    Ok(())
}
