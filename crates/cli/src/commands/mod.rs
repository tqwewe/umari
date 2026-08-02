#[allow(clippy::module_inception)]
pub mod commands;
pub mod deploy_ui;
pub mod effects;
pub mod env_vars;
pub mod execute;
pub mod init;
pub mod modules;
pub mod new;
pub mod projectors;
pub mod workspace;

use std::io::IsTerminal;

use anyhow::Result;
use dialoguer::{Confirm, theme::ColorfulTheme};

/// Prompt the user to confirm a destructive action. Returns `true` without
/// prompting when stdin is not a terminal, so scripted deletes don't hang.
pub fn confirm_destructive(prompt: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Ok(true);
    }
    Ok(Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(false)
        .interact()?)
}
