mod client;
mod commands;
mod output;

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
        env,
        default_value = "http://localhost:3000"
    )]
    url: String,

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
    /// manage policy modules
    Policies {
        #[command(subcommand)]
        command: PoliciesSubcommand,
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
    },
    /// build and deploy wasm modules to the server
    Deploy {
        #[arg(value_name = "PATHS")]
        paths: Vec<PathBuf>,
        /// upload without activating
        #[arg(long)]
        no_activate: bool,
        #[arg(long)]
        debug: bool,
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
enum PoliciesSubcommand {
    /// upload a policy module
    Upload {
        /// module name
        name: String,
        /// semantic version (e.g., 1.0.0)
        version: String,
        /// WASM file path
        file: PathBuf,
        /// activate immediately after upload
        #[arg(long)]
        activate: bool,
    },
    /// list policy modules
    List {
        /// show only active modules
        #[arg(long)]
        active_only: bool,
        /// filter by module name
        #[arg(long)]
        name: Option<String>,
    },
    /// show policy module details
    Show {
        /// module name
        name: String,
        /// specific version (optional)
        version: Option<String>,
    },
    /// activate a policy version
    Activate {
        /// module name
        name: String,
        /// version to activate
        version: String,
    },
    /// deactivate a policy module
    Deactivate {
        /// module name
        name: String,
    },
    /// reset and replay a policy module from position 0
    Replay {
        /// module name
        name: String,
    },
    /// manage environment variables for a policy module
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

#[derive(Subcommand)]
enum NewSubcommand {
    /// create a new command module
    Command { name: String },
    /// create a new projector module
    Projector { name: String },
    /// create a new policy module
    Policy { name: String },
    /// create a new effect module
    Effect { name: String },
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = ApiClient::new(cli.url);

    match cli.command {
        Commands::Commands { command } => match command {
            CommandsSubcommand::Upload {
                name,
                version,
                file,
                activate,
            } => commands::commands::upload(&client, name, version, file, activate),
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
                file,
                activate,
            } => commands::projectors::upload(&client, name, version, file, activate),
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
        Commands::Policies { command } => match command {
            PoliciesSubcommand::Upload {
                name,
                version,
                file,
                activate,
            } => commands::policies::upload(&client, name, version, file, activate),
            PoliciesSubcommand::List { active_only, name } => {
                commands::policies::list(&client, active_only, name)
            }
            PoliciesSubcommand::Show { name, version } => {
                commands::policies::show(&client, name, version)
            }
            PoliciesSubcommand::Activate { name, version } => {
                commands::policies::activate(&client, name, version)
            }
            PoliciesSubcommand::Deactivate { name } => {
                commands::policies::deactivate(&client, name)
            }
            PoliciesSubcommand::Replay { name } => commands::policies::replay(&client, name),
            PoliciesSubcommand::Env { name, action } => match action {
                EnvAction::List => commands::env_vars::list(&client, "policies", &name),
                EnvAction::Set { key, value } => {
                    commands::env_vars::set(&client, "policies", &name, &key, &value)
                }
                EnvAction::Unset { key } => {
                    commands::env_vars::unset(&client, "policies", &name, &key)
                }
            },
        },
        Commands::Effects { command } => match command {
            EffectsSubcommand::Upload {
                name,
                version,
                file,
                activate,
            } => commands::effects::upload(&client, name, version, file, activate),
            EffectsSubcommand::List { active_only, name } => {
                commands::effects::list(&client, active_only, name)
            }
            EffectsSubcommand::Show { name, version } => {
                commands::effects::show(&client, name, version)
            }
            EffectsSubcommand::Activate { name, version } => {
                commands::effects::activate(&client, name, version)
            }
            EffectsSubcommand::Deactivate { name } => {
                commands::effects::deactivate(&client, name)
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
        Commands::Build { paths, debug } => commands::workspace::build(paths, debug),
        Commands::Deploy {
            paths,
            no_activate,
            debug,
        } => commands::workspace::deploy(&client, paths, no_activate, debug),
        Commands::New { command } => match command {
            NewSubcommand::Command { name } => commands::new::generate("command", &name),
            NewSubcommand::Projector { name } => commands::new::generate("projector", &name),
            NewSubcommand::Policy { name } => commands::new::generate("policy", &name),
            NewSubcommand::Effect { name } => commands::new::generate("effect", &name),
        },
    }
}
