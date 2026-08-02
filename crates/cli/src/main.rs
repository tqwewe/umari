mod client;
mod commands;
mod output;

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use client::ApiClient;

#[derive(Parser)]
#[command(name = "umari", version, about = "umari event-sourcing CLI")]
struct Cli {
    /// server URL (overrides UMARI_URL env var)
    #[arg(
        long,
        short,
        global = true,
        env = "UMARI_URL",
        default_value = "http://localhost:3000"
    )]
    url: String,

    /// API key for authentication (overrides UMARI_API_KEY env var)
    #[arg(long, global = true, env = "UMARI_API_KEY")]
    api_key: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// manage command modules
    #[allow(clippy::enum_variant_names)]
    Commands {
        #[command(subcommand)]
        command: CommandsSubcommand,
    },
    /// manage projector modules
    Projectors {
        #[command(subcommand)]
        command: ProjectorsSubcommand,
    },
    /// manage effect modules
    Effects {
        #[command(subcommand)]
        command: EffectsSubcommand,
    },
    /// view active modules
    Modules {
        #[command(subcommand)]
        command: ModulesSubcommand,
    },
    /// execute a command
    Execute {
        /// command name
        name: String,
        /// input JSON string
        #[arg(long)]
        input: String,
    },
    /// build wasm modules in the workspace
    Build {
        #[arg(value_name = "PATHS")]
        paths: Vec<PathBuf>,
        #[arg(long)]
        debug: bool,
        /// max parallel builds (cargo -j, JS worker count, upload worker count); 0 = auto
        #[arg(long, short = 'j', default_value_t = 0)]
        jobs: usize,
    },
    /// build and deploy wasm modules to the server
    Deploy {
        #[arg(value_name = "PATHS")]
        paths: Vec<PathBuf>,
        /// upload without activating
        #[arg(long)]
        no_activate: bool,
        /// automatically bump the patch version and retry when a module already exists
        #[arg(long)]
        bump_patch: bool,
        #[arg(long)]
        debug: bool,
        /// max parallel builds (cargo -j, JS worker count, upload worker count); 0 = auto
        #[arg(long, short = 'j', default_value_t = 0)]
        jobs: usize,
    },
    /// scaffold a new umari workspace
    Init {
        /// directory to create (defaults to the current directory)
        path: Option<String>,
        #[arg(long)]
        lang: Option<Lang>,
    },
    /// scaffold a new module in the workspace
    New {
        #[command(subcommand)]
        command: NewSubcommand,
    },
}

#[derive(Subcommand)]
enum CommandsSubcommand {
    /// upload a command module
    Upload {
        /// module name
        name: String,
        /// semantic version (e.g., 1.0.0)
        version: String,
        /// environment variables
        #[arg(long = "env", value_parser = parse_key_val)]
        env: Vec<(String, String)>,
        /// WASM file path
        file: PathBuf,
        /// activate immediately after upload
        #[arg(long)]
        activate: bool,
    },
    /// list command modules
    List {
        /// show only active modules
        #[arg(long)]
        active_only: bool,
        /// filter by module name
        #[arg(long)]
        name: Option<String>,
    },
    /// show command module details
    Show {
        /// module name
        name: String,
        /// specific version (optional)
        version: Option<String>,
    },
    /// activate a command version
    Activate {
        /// module name
        name: String,
        /// version to activate
        version: String,
    },
    /// deactivate a command module
    Deactivate {
        /// module name
        name: String,
    },
    /// delete a command module, or a single version when one is given
    Delete {
        /// module name
        name: String,
        /// specific version to delete (deletes the whole module when omitted)
        version: Option<String>,
        /// skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// manage environment variables for a command module
    Env {
        /// module name
        name: String,
        #[command(subcommand)]
        action: EnvAction,
    },
}

#[derive(Subcommand)]
enum ProjectorsSubcommand {
    /// upload a projector module
    Upload {
        /// module name
        name: String,
        /// semantic version (e.g., 1.0.0)
        version: String,
        /// environment variables
        #[arg(long = "env", value_parser = parse_key_val)]
        env: Vec<(String, String)>,
        /// WASM file path
        file: PathBuf,
        /// activate immediately after upload
        #[arg(long)]
        activate: bool,
    },
    /// list projector modules
    List {
        /// show only active modules
        #[arg(long)]
        active_only: bool,
        /// filter by module name
        #[arg(long)]
        name: Option<String>,
    },
    /// show projector module details
    Show {
        /// module name
        name: String,
        /// specific version (optional)
        version: Option<String>,
    },
    /// activate a projector version
    Activate {
        /// module name
        name: String,
        /// version to activate
        version: String,
    },
    /// deactivate a projector module
    Deactivate {
        /// module name
        name: String,
    },
    /// delete a projector module, or a single version when one is given
    Delete {
        /// module name
        name: String,
        /// specific version to delete (deletes the whole module when omitted)
        version: Option<String>,
        /// skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// reset and replay a projector module from position 0
    Replay {
        /// module name
        name: String,
    },
    /// manage environment variables for a projector module
    Env {
        /// module name
        name: String,
        #[command(subcommand)]
        action: EnvAction,
    },
}

#[derive(Subcommand)]
enum EffectsSubcommand {
    /// upload an effect module
    Upload {
        /// module name
        name: String,
        /// semantic version (e.g., 1.0.0)
        version: String,
        /// environment variables
        #[arg(long = "env", value_parser = parse_key_val)]
        env: Vec<(String, String)>,
        /// WASM file path
        file: PathBuf,
        /// activate immediately after upload
        #[arg(long)]
        activate: bool,
    },
    /// list effect modules
    List {
        /// show only active modules
        #[arg(long)]
        active_only: bool,
        /// filter by module name
        #[arg(long)]
        name: Option<String>,
    },
    /// show effect module details
    Show {
        /// module name
        name: String,
        /// specific version (optional)
        version: Option<String>,
    },
    /// activate an effect version
    Activate {
        /// module name
        name: String,
        /// version to activate
        version: String,
    },
    /// deactivate an effect module
    Deactivate {
        /// module name
        name: String,
    },
    /// delete an effect module, or a single version when one is given
    Delete {
        /// module name
        name: String,
        /// specific version to delete (deletes the whole module when omitted)
        version: Option<String>,
        /// skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// reset and replay an effect module from position 0
    Replay {
        /// module name
        name: String,
    },
    /// manage environment variables for an effect module
    Env {
        /// module name
        name: String,
        #[command(subcommand)]
        action: EnvAction,
    },
}

#[derive(Subcommand, Clone)]
enum EnvAction {
    /// list all environment variables
    List,
    /// set an environment variable
    Set {
        /// variable key
        key: String,
        /// variable value
        value: String,
    },
    /// unset an environment variable
    Unset {
        /// variable key
        key: String,
    },
}

#[derive(clap::ValueEnum, Clone)]
enum Lang {
    Rust,
    Js,
}

#[derive(Subcommand)]
enum NewSubcommand {
    /// create a new command module
    Command {
        name: String,
        #[arg(long)]
        lang: Option<Lang>,
    },
    /// create a new projector module
    Projector {
        name: String,
        #[arg(long)]
        lang: Option<Lang>,
    },
    /// create a new effect module
    Effect {
        name: String,
        #[arg(long)]
        lang: Option<Lang>,
    },
}

/// resolve the language for `init`, prompting interactively when not passed
/// via --lang. init creates a fresh workspace, so there is nothing to infer.
fn resolve_lang(lang: Option<Lang>) -> Result<Lang> {
    match lang {
        Some(lang) => Ok(lang),
        None => prompt_lang(),
    }
}

/// resolve the language for `new`: an explicit --lang wins, otherwise infer it
/// from the surrounding workspace, and only prompt when there is no workspace.
fn resolve_lang_for_new(lang: Option<Lang>) -> Result<Lang> {
    if let Some(lang) = lang {
        return Ok(lang);
    }
    if let Some(lang) = detect_workspace_lang() {
        return Ok(lang);
    }
    prompt_lang()
}

/// Walk up from the current directory looking for a workspace marker: a Rust
/// `[workspace]` Cargo.toml or a `package.json` with a `workspaces` field.
fn detect_workspace_lang() -> Option<Lang> {
    let cwd = std::env::current_dir().ok()?;
    for dir in cwd.ancestors() {
        if let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml"))
            && content.contains("[workspace]") {
                return Some(Lang::Rust);
            }
        if let Ok(content) = std::fs::read_to_string(dir.join("package.json"))
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
                && json.get("workspaces").is_some() {
                    return Some(Lang::Js);
                }
    }
    None
}

fn prompt_lang() -> Result<Lang> {
    // no tty (scripts, CI) — keep the historical default rather than hang.
    if !std::io::stdin().is_terminal() {
        return Ok(Lang::Rust);
    }
    let items = ["rust", "javascript / typescript"];
    let selection = dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt("which language?")
        .items(items)
        .default(0)
        .interact()?;
    Ok(match selection {
        1 => Lang::Js,
        _ => Lang::Rust,
    })
}

#[derive(Subcommand)]
enum ModulesSubcommand {
    /// list all active modules
    Active {
        /// filter by module type
        #[arg(long)]
        r#type: Option<String>,
    },
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let (k, v) = s.split_once('=').ok_or("expected KEY=VALUE")?;

    Ok((k.to_string(), v.to_string()))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = ApiClient::new(cli.url, cli.api_key);

    match cli.command {
        Commands::Commands { command } => match command {
            CommandsSubcommand::Upload {
                name,
                version,
                env,
                file,
                activate,
            } => commands::commands::upload(
                &client,
                name,
                version,
                env.into_iter().collect(),
                file,
                activate,
            ),
            CommandsSubcommand::List { active_only, name } => {
                commands::commands::list(&client, active_only, name)
            }
            CommandsSubcommand::Show { name, version } => {
                commands::commands::show(&client, name, version)
            }
            CommandsSubcommand::Activate { name, version } => {
                commands::commands::activate(&client, name, version)
            }
            CommandsSubcommand::Deactivate { name } => {
                commands::commands::deactivate(&client, name)
            }
            CommandsSubcommand::Delete { name, version, yes } => {
                commands::commands::delete(&client, name, version, yes)
            }
            CommandsSubcommand::Env { name, action } => match action {
                EnvAction::List => commands::env_vars::list(&client, "commands", &name),
                EnvAction::Set { key, value } => {
                    commands::env_vars::set(&client, "commands", &name, &key, &value)
                }
                EnvAction::Unset { key } => {
                    commands::env_vars::unset(&client, "commands", &name, &key)
                }
            },
        },
        Commands::Projectors { command } => match command {
            ProjectorsSubcommand::Upload {
                name,
                version,
                env,
                file,
                activate,
            } => commands::projectors::upload(
                &client,
                name,
                version,
                env.into_iter().collect(),
                file,
                activate,
            ),
            ProjectorsSubcommand::List { active_only, name } => {
                commands::projectors::list(&client, active_only, name)
            }
            ProjectorsSubcommand::Show { name, version } => {
                commands::projectors::show(&client, name, version)
            }
            ProjectorsSubcommand::Activate { name, version } => {
                commands::projectors::activate(&client, name, version)
            }
            ProjectorsSubcommand::Deactivate { name } => {
                commands::projectors::deactivate(&client, name)
            }
            ProjectorsSubcommand::Delete { name, version, yes } => {
                commands::projectors::delete(&client, name, version, yes)
            }
            ProjectorsSubcommand::Replay { name } => commands::projectors::replay(&client, name),
            ProjectorsSubcommand::Env { name, action } => match action {
                EnvAction::List => commands::env_vars::list(&client, "projectors", &name),
                EnvAction::Set { key, value } => {
                    commands::env_vars::set(&client, "projectors", &name, &key, &value)
                }
                EnvAction::Unset { key } => {
                    commands::env_vars::unset(&client, "projectors", &name, &key)
                }
            },
        },
        Commands::Effects { command } => match command {
            EffectsSubcommand::Upload {
                name,
                version,
                env,
                file,
                activate,
            } => commands::effects::upload(
                &client,
                name,
                version,
                env.into_iter().collect(),
                file,
                activate,
            ),
            EffectsSubcommand::List { active_only, name } => {
                commands::effects::list(&client, active_only, name)
            }
            EffectsSubcommand::Show { name, version } => {
                commands::effects::show(&client, name, version)
            }
            EffectsSubcommand::Activate { name, version } => {
                commands::effects::activate(&client, name, version)
            }
            EffectsSubcommand::Deactivate { name } => commands::effects::deactivate(&client, name),
            EffectsSubcommand::Delete { name, version, yes } => {
                commands::effects::delete(&client, name, version, yes)
            }
            EffectsSubcommand::Replay { name } => commands::effects::replay(&client, name),
            EffectsSubcommand::Env { name, action } => match action {
                EnvAction::List => commands::env_vars::list(&client, "effects", &name),
                EnvAction::Set { key, value } => {
                    commands::env_vars::set(&client, "effects", &name, &key, &value)
                }
                EnvAction::Unset { key } => {
                    commands::env_vars::unset(&client, "effects", &name, &key)
                }
            },
        },
        Commands::Modules { command } => match command {
            ModulesSubcommand::Active { r#type } => commands::modules::active(&client, r#type),
        },
        Commands::Execute { name, input } => commands::execute::execute(&client, name, input),
        Commands::Build { paths, debug, jobs } => {
            commands::workspace::build(paths, debug, jobs)
        }
        Commands::Deploy {
            paths,
            no_activate,
            bump_patch,
            debug,
            jobs,
        } => commands::workspace::deploy(&client, paths, no_activate, bump_patch, debug, jobs),
        Commands::New { command } => {
            let (module_type, name, lang) = match command {
                NewSubcommand::Command { name, lang } => ("command", name, lang),
                NewSubcommand::Projector { name, lang } => ("projector", name, lang),
                NewSubcommand::Effect { name, lang } => ("effect", name, lang),
            };
            match resolve_lang_for_new(lang)? {
                Lang::Js => commands::new::generate_js(module_type, &name),
                Lang::Rust => commands::new::generate(module_type, &name),
            }
        }
        Commands::Init { path, lang } => {
            let path = path.as_deref();
            match resolve_lang(lang)? {
                Lang::Js => commands::init::init_js(path),
                Lang::Rust => commands::init::init_rust(path),
            }
        }
    }
}
