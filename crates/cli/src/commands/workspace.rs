use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use semver::Version;
use serde::Deserialize;

use crate::client::ApiClient;
use crate::commands::deploy_ui::{DeployUi, ModuleType, Stats, Status};

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
    metadata: Option<CargoPackageMetadata>,
}

#[derive(Deserialize)]
struct CargoTarget {
    kind: Vec<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct CargoPackageMetadata {
    #[serde(default)]
    umari: UmariMetadata,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct UmariMetadata {
    #[serde(default)]
    env: BTreeMap<String, String>,
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

fn detect_module_type(path: &Path) -> Option<ModuleType> {
    for component in path.components() {
        match component.as_os_str().to_str()? {
            "commands" => return Some(ModuleType::Commands),
            "projectors" => return Some(ModuleType::Projectors),
            "effects" => return Some(ModuleType::Effects),
            _ => {}
        }
    }
    None
}

struct Module {
    name: String,
    version: String,
    module_type: ModuleType,
    env_vars: BTreeMap<String, String>,
    wasm_path: PathBuf,
    manifest_path: PathBuf,
}

struct JsModule {
    name: String,
    version: String,
    module_type: ModuleType,
    env_vars: BTreeMap<String, String>,
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

    fn module_type(&self) -> ModuleType {
        match self {
            AnyModule::Rust(m) => m.module_type,
            AnyModule::Js(m) => m.module_type,
        }
    }

    fn env_vars(&self) -> &BTreeMap<String, String> {
        match self {
            AnyModule::Rust(m) => &m.env_vars,
            AnyModule::Js(m) => &m.env_vars,
        }
    }

    fn wasm_path(&self) -> &Path {
        match self {
            AnyModule::Rust(m) => &m.wasm_path,
            AnyModule::Js(m) => &m.wasm_path,
        }
    }

    fn manifest_path(&self) -> PathBuf {
        match self {
            AnyModule::Rust(m) => m.manifest_path.clone(),
            AnyModule::Js(m) => m.dir.join("package.json"),
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
    for pkg in metadata.packages {
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
        let env_vars = pkg.metadata.unwrap_or_default().umari.env;

        modules.push(AnyModule::Rust(Module {
            name: pkg.name,
            version: pkg.version,
            module_type,
            env_vars,
            wasm_path,
            manifest_path: pkg.manifest_path,
        }));
    }

    // JS modules
    for type_dir in &["commands", "projectors", "effects"] {
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
                env_vars: BTreeMap::new(),
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

    // Stable sort by (type, name) for grouped display.
    modules.sort_by(|a, b| {
        a.module_type()
            .cmp(&b.module_type())
            .then_with(|| a.name().cmp(b.name()))
    });

    Ok((modules, metadata.workspace_root))
}

fn name_width(modules: &[AnyModule]) -> usize {
    modules.iter().map(|m| m.name().len()).max().unwrap_or(20)
}

fn version_width(modules: &[AnyModule]) -> usize {
    modules.iter().map(|m| m.version().len()).max().unwrap_or(6)
}

fn register_phase_rows(ui: &DeployUi, modules: &[AnyModule], action: &'static str) {
    let mut by_type: BTreeMap<ModuleType, Vec<&AnyModule>> = BTreeMap::new();
    for m in modules {
        by_type.entry(m.module_type()).or_default().push(m);
    }
    for (ty, group) in &by_type {
        ui.add_category_header(*ty, group.len());
        for m in group {
            ui.register(*ty, m.name(), m.version(), action);
        }
    }
}

pub fn build(paths: Vec<PathBuf>, debug: bool, jobs: usize) -> Result<()> {
    let (modules, _workspace_root) = discover_modules(&paths, debug)?;

    if modules.is_empty() {
        return Err(anyhow!("no wasm modules found"));
    }

    let ui = DeployUi::new(name_width(&modules), version_width(&modules));
    ui.begin_phase("Building", modules.len());
    register_phase_rows(&ui, &modules, "building…");

    let start = Instant::now();
    let (stats, _) = run_build_phase(&ui, &modules, debug, jobs, /*finalize=*/ true);
    ui.print_summary(&stats, start.elapsed(), "Built", modules.len());

    let total_failed: usize = stats.values().map(|s| s.failed).sum();
    if total_failed > 0 {
        return Err(anyhow!("{total_failed} module(s) failed to build"));
    }
    Ok(())
}

pub fn deploy(
    client: &ApiClient,
    paths: Vec<PathBuf>,
    no_activate: bool,
    bump_patch: bool,
    debug: bool,
    jobs: usize,
) -> Result<()> {
    let (modules, _workspace_root) = discover_modules(&paths, debug)?;

    if modules.is_empty() {
        return Err(anyhow!("no wasm modules found"));
    }

    let ui = DeployUi::new(name_width(&modules), version_width(&modules));
    let total_start = Instant::now();

    // ONE unified list — register every module once, then transition each row
    // through "building…" → "uploading…" → final state.
    ui.begin_phase("Deploying", modules.len());
    register_phase_rows(&ui, &modules, "building…");

    let (build_stats, alive) =
        run_build_phase(&ui, &modules, debug, jobs, /*finalize=*/ false);

    let upload_stats = run_upload_phase(
        client,
        &ui,
        &alive,
        no_activate,
        bump_patch,
        debug,
        jobs,
    );

    // Merge build failures into the upload stats so the summary shows the full
    // picture across both phases.
    let mut combined = upload_stats;
    for (ty, s) in &build_stats {
        let entry = combined.entry(*ty).or_default();
        entry.failed += s.failed;
    }

    ui.print_summary(&combined, total_start.elapsed(), "Deployed", modules.len());

    let total_failed: usize = combined.values().map(|s| s.failed).sum();
    if total_failed > 0 {
        return Err(anyhow!("{total_failed} module(s) failed to deploy"));
    }
    Ok(())
}

// -------- build phase --------

fn run_build_phase<'a>(
    ui: &DeployUi,
    modules: &'a [AnyModule],
    debug: bool,
    jobs: usize,
    finalize: bool,
) -> (BTreeMap<ModuleType, Stats>, Vec<&'a AnyModule>) {
    let stats: Mutex<BTreeMap<ModuleType, Stats>> = Mutex::new(init_stats(modules));
    // (ModuleType, name) keys for modules that built successfully — used to
    // filter the alive list below.
    let succeeded: Mutex<std::collections::HashSet<(ModuleType, String)>> =
        Mutex::new(std::collections::HashSet::new());

    let rust_modules: Vec<&Module> = modules
        .iter()
        .filter_map(|m| match m {
            AnyModule::Rust(r) => Some(r),
            _ => None,
        })
        .collect();
    let js_modules: Vec<&JsModule> = modules
        .iter()
        .filter_map(|m| match m {
            AnyModule::Js(j) => Some(j),
            _ => None,
        })
        .collect();

    let js_queue: Mutex<Vec<&JsModule>> = Mutex::new(js_modules.iter().copied().collect());
    let resolved_jobs = resolve_jobs(jobs);
    let js_workers = resolved_jobs.min(js_modules.len().max(1));

    thread::scope(|s| {
        if !rust_modules.is_empty() {
            let stats = &stats;
            let succeeded = &succeeded;
            let rust_ref = &rust_modules;
            s.spawn(move || {
                run_rust_build(ui, rust_ref, debug, jobs, finalize, stats, succeeded);
            });
        }

        if !js_modules.is_empty() {
            for _ in 0..js_workers {
                let queue = &js_queue;
                let stats = &stats;
                let succeeded = &succeeded;
                s.spawn(move || loop {
                    let m = match queue.lock().unwrap().pop() {
                        Some(m) => m,
                        None => break,
                    };
                    let start = Instant::now();
                    match build_one_js(m) {
                        Ok(()) => {
                            if finalize {
                                ui.finish(
                                    m.module_type,
                                    &m.name,
                                    Status::Built {
                                        dur: start.elapsed(),
                                    },
                                );
                            } else {
                                ui.set_action(m.module_type, &m.name, "uploading…");
                            }
                            succeeded
                                .lock()
                                .unwrap()
                                .insert((m.module_type, m.name.clone()));
                        }
                        Err(err) => {
                            ui.finish(
                                m.module_type,
                                &m.name,
                                Status::Failed {
                                    msg: err.to_string(),
                                },
                            );
                            stats.lock().unwrap().entry(m.module_type).or_default().failed += 1;
                        }
                    }
                });
            }
        }
    });

    let succeeded = succeeded.into_inner().unwrap();
    let alive: Vec<&AnyModule> = modules
        .iter()
        .filter(|m| succeeded.contains(&(m.module_type(), m.name().to_string())))
        .collect();

    (stats.into_inner().unwrap(), alive)
}

fn run_rust_build(
    ui: &DeployUi,
    modules: &[&Module],
    debug: bool,
    jobs: usize,
    finalize: bool,
    stats: &Mutex<BTreeMap<ModuleType, Stats>>,
    succeeded: &Mutex<std::collections::HashSet<(ModuleType, String)>>,
) {
    // Map underscored crate name → (module_type, package_name, start_instant).
    let start_map: HashMap<String, (ModuleType, String, Instant)> = modules
        .iter()
        .map(|m| {
            (
                m.name.replace('-', "_"),
                (m.module_type, m.name.clone(), Instant::now()),
            )
        })
        .collect();

    let mut cmd = Command::new("cargo");
    cmd.args([
        "build",
        "--target",
        "wasm32-wasip2",
        "--message-format=json-render-diagnostics",
    ]);
    if !debug {
        cmd.arg("--release");
    }
    if jobs > 0 {
        cmd.arg("-j").arg(jobs.to_string());
    }
    for m in modules {
        cmd.arg("-p").arg(&m.name);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(err) => {
            for m in modules {
                ui.finish(
                    m.module_type,
                    &m.name,
                    Status::Failed {
                        msg: format!("failed to spawn cargo: {err}"),
                    },
                );
                stats.lock().unwrap().entry(m.module_type).or_default().failed += 1;
            }
            return;
        }
    };

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let mut finished: std::collections::HashSet<String> = std::collections::HashSet::new();
    let errors: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());

    let stderr_handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut out = String::new();
        for line in reader.lines().map_while(|l| l.ok()) {
            out.push_str(&line);
            out.push('\n');
        }
        out
    });

    let reader = BufReader::new(stdout);
    for line in reader.lines().map_while(|l| l.ok()) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let reason = value.get("reason").and_then(|v| v.as_str()).unwrap_or("");
        match reason {
            "compiler-artifact" => {
                let target = value.get("target");
                let kinds = target
                    .and_then(|t| t.get("kind"))
                    .and_then(|k| k.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if !kinds.iter().any(|k| k == "cdylib") {
                    continue;
                }
                let name = target
                    .and_then(|t| t.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                if let Some((ty, pkg_name, started)) = start_map.get(name) {
                    if finished.insert(name.to_string()) {
                        if finalize {
                            ui.finish(
                                *ty,
                                pkg_name,
                                Status::Built {
                                    dur: started.elapsed(),
                                },
                            );
                        } else {
                            ui.set_action(*ty, pkg_name, "uploading…");
                        }
                        succeeded.lock().unwrap().insert((*ty, pkg_name.clone()));
                    }
                }
            }
            "compiler-message" => {
                let level = value
                    .get("message")
                    .and_then(|m| m.get("level"))
                    .and_then(|l| l.as_str())
                    .unwrap_or("");
                if level != "error" {
                    continue;
                }
                let target_name = value
                    .get("target")
                    .and_then(|t| t.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let rendered = value
                    .get("message")
                    .and_then(|m| m.get("rendered"))
                    .and_then(|r| r.as_str())
                    .unwrap_or("compile error")
                    .to_string();
                if !target_name.is_empty() {
                    errors
                        .lock()
                        .unwrap()
                        .entry(target_name.to_string())
                        .or_insert(first_line(&rendered));
                }
            }
            _ => {}
        }
    }

    let exit_status = child.wait();
    let stderr_output = stderr_handle.join().unwrap_or_default();

    let success = matches!(&exit_status, Ok(s) if s.success());

    // Anything not finished is a failure.
    for m in modules {
        let crate_name = m.name.replace('-', "_");
        if finished.contains(&crate_name) {
            continue;
        }
        let msg = errors
            .lock()
            .unwrap()
            .remove(&crate_name)
            .or_else(|| {
                if !success && !stderr_output.trim().is_empty() {
                    Some(first_line(&stderr_output))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "build failed".to_string());
        ui.finish(
            m.module_type,
            &m.name,
            Status::Failed { msg },
        );
        stats.lock().unwrap().entry(m.module_type).or_default().failed += 1;
    }
}

fn build_one_js(module: &JsModule) -> Result<()> {
    let output = Command::new("npm")
        .args(["run", "build"])
        .current_dir(&module.dir)
        .output()
        .map_err(|err| anyhow!("failed to run npm: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("{}", first_line(&stderr)));
    }
    Ok(())
}

fn build_one_rust(module: &Module, debug: bool) -> Result<()> {
    let mut args = vec!["build", "-p", &module.name, "--target", "wasm32-wasip2"];
    if !debug {
        args.push("--release");
    }
    let output = Command::new("cargo")
        .args(&args)
        .output()
        .map_err(|err| anyhow!("failed to run cargo build: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("{}", first_line(&stderr)));
    }
    Ok(())
}

fn rebuild_module(module: &AnyModule, debug: bool) -> Result<()> {
    match module {
        AnyModule::Rust(m) => build_one_rust(m, debug),
        AnyModule::Js(m) => build_one_js(m),
    }
}

// -------- upload phase --------

fn run_upload_phase(
    client: &ApiClient,
    ui: &DeployUi,
    modules: &[&AnyModule],
    no_activate: bool,
    bump_patch: bool,
    debug: bool,
    jobs: usize,
) -> BTreeMap<ModuleType, Stats> {
    let stats: Mutex<BTreeMap<ModuleType, Stats>> =
        Mutex::new(init_stats_from_refs(modules));
    let queue: Mutex<Vec<&AnyModule>> = Mutex::new(modules.iter().copied().collect());
    let workers = resolve_jobs(jobs).min(modules.len().max(1)).max(1);

    thread::scope(|s| {
        for _ in 0..workers {
            let queue = &queue;
            let stats = &stats;
            s.spawn(move || loop {
                let m = match queue.lock().unwrap().pop() {
                    Some(m) => m,
                    None => break,
                };
                let status = upload_one(client, m, no_activate, bump_patch, debug);
                let ty = m.module_type();
                {
                    let mut g = stats.lock().unwrap();
                    let entry = g.entry(ty).or_default();
                    match &status {
                        Status::Built { .. } => {} // not used in upload
                        Status::Unchanged => entry.unchanged += 1,
                        Status::Deployed => entry.deployed += 1,
                        Status::Bumped { .. } => entry.bumped += 1,
                        Status::Failed { .. } => entry.failed += 1,
                    }
                }
                ui.finish(ty, m.name(), status);
            });
        }
    });

    stats.into_inner().unwrap()
}

fn upload_one(
    client: &ApiClient,
    module: &AnyModule,
    no_activate: bool,
    bump_patch: bool,
    debug: bool,
) -> Status {
    let wasm_path = module.wasm_path();
    if !wasm_path.exists() {
        return Status::Failed {
            msg: format!("wasm file missing: {}", wasm_path.display()),
        };
    }

    let version = module.version().to_string();
    let result = client.upload_wasm(
        module.module_type().as_str(),
        module.name(),
        &version,
        module.env_vars(),
        wasm_path,
        !no_activate,
    );

    match result {
        Ok(Some((idempotent, _))) => {
            if idempotent {
                Status::Unchanged
            } else {
                Status::Deployed
            }
        }
        Ok(None) if bump_patch => {
            let new_version = match bump_patch_version(&version) {
                Ok(v) => v,
                Err(err) => return Status::Failed { msg: err.to_string() },
            };
            if let Err(err) = write_version_to_manifest(module, &new_version) {
                return Status::Failed { msg: err.to_string() };
            }
            if let Err(err) = rebuild_module(module, debug) {
                return Status::Failed { msg: err.to_string() };
            }
            match client.upload_wasm(
                module.module_type().as_str(),
                module.name(),
                &new_version,
                module.env_vars(),
                wasm_path,
                !no_activate,
            ) {
                Ok(Some(_)) => Status::Bumped {
                    from: version,
                    to: new_version,
                },
                Ok(None) => Status::Failed {
                    msg: "module already exists after bump".to_string(),
                },
                Err(err) => Status::Failed { msg: err.to_string() },
            }
        }
        Ok(None) => Status::Failed {
            msg: "module already exists".to_string(),
        },
        Err(err) => Status::Failed { msg: err.to_string() },
    }
}

// -------- helpers --------

fn resolve_jobs(jobs: usize) -> usize {
    if jobs == 0 {
        thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    } else {
        jobs
    }
}

fn init_stats(modules: &[AnyModule]) -> BTreeMap<ModuleType, Stats> {
    let mut map = BTreeMap::new();
    for m in modules {
        map.entry(m.module_type()).or_insert(Stats::default());
    }
    map
}

fn init_stats_from_refs(modules: &[&AnyModule]) -> BTreeMap<ModuleType, Stats> {
    let mut map = BTreeMap::new();
    for m in modules {
        map.entry(m.module_type()).or_insert(Stats::default());
    }
    map
}

fn first_line(s: &str) -> String {
    s.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(s)
        .to_string()
}

fn bump_patch_version(version: &str) -> Result<String> {
    let mut v: Version = version.parse().context("invalid semver version")?;
    v.patch += 1;
    Ok(v.to_string())
}

fn write_version_to_manifest(module: &AnyModule, new_version: &str) -> Result<()> {
    let manifest_path = module.manifest_path();
    match module {
        AnyModule::Rust(_) => {
            let content = std::fs::read_to_string(&manifest_path)
                .with_context(|| format!("failed to read {}", manifest_path.display()))?;
            let mut doc: toml_edit::DocumentMut = content
                .parse()
                .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
            doc["package"]["version"] = toml_edit::value(new_version);
            std::fs::write(&manifest_path, doc.to_string())
                .with_context(|| format!("failed to write {}", manifest_path.display()))?;
        }
        AnyModule::Js(_) => {
            let content = std::fs::read_to_string(&manifest_path)
                .with_context(|| format!("failed to read {}", manifest_path.display()))?;
            let mut pkg: serde_json::Value = serde_json::from_str(&content)
                .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
            pkg["version"] = serde_json::Value::String(new_version.to_string());
            let updated =
                serde_json::to_string_pretty(&pkg).context("failed to serialize package.json")?;
            std::fs::write(&manifest_path, updated + "\n")
                .with_context(|| format!("failed to write {}", manifest_path.display()))?;
        }
    }
    Ok(())
}

