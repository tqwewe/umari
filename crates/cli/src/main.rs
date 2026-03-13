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
    #[arg(long, short, global = true, env, default_value = "http://localhost:3000")]
    url: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// manage command modules
    Commands {
        #[command(subcommand)]
        command: CommandsSubcommand,
    },
    /// manage projection modules
    Projections {
        #[command(subcommand)]
        command: ProjectionsSubcommand,
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
}

#[derive(Subcommand)]
enum ProjectionsSubcommand {
    /// upload a projection module
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
    /// list projection modules
    List {
        /// show only active modules
        #[arg(long)]
        active_only: bool,
        /// filter by module name
        #[arg(long)]
        name: Option<String>,
    },
    /// show projection module details
    Show {
        /// module name
        name: String,
        /// specific version (optional)
        version: Option<String>,
    },
    /// activate a projection version
    Activate {
        /// module name
        name: String,
        /// version to activate
        version: String,
    },
    /// deactivate a projection module
    Deactivate {
        /// module name
        name: String,
    },
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
            CommandsSubcommand::Upload { name, version, file, activate } => {
                commands::commands::upload(&client, name, version, file, activate)
            }
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
        },
        Commands::Projections { command } => match command {
            ProjectionsSubcommand::Upload { name, version, file, activate } => {
                commands::projections::upload(&client, name, version, file, activate)
            }
            ProjectionsSubcommand::List { active_only, name } => {
                commands::projections::list(&client, active_only, name)
            }
            ProjectionsSubcommand::Show { name, version } => {
                commands::projections::show(&client, name, version)
            }
            ProjectionsSubcommand::Activate { name, version } => {
                commands::projections::activate(&client, name, version)
            }
            ProjectionsSubcommand::Deactivate { name } => {
                commands::projections::deactivate(&client, name)
            }
        },
        Commands::Modules { command } => match command {
            ModulesSubcommand::Active { r#type } => {
                commands::modules::active(&client, r#type)
            }
        },
        Commands::Execute { name, input } => {
            commands::execute::execute(&client, name, input)
        }
    }
}
