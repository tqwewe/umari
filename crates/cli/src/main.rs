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
        },
        Commands::Modules { command } => match command {
            ModulesSubcommand::Active { r#type } => commands::modules::active(&client, r#type),
        },
        Commands::Execute { name, input } => commands::execute::execute(&client, name, input),
    }
}
