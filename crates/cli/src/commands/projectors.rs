use std::path::PathBuf;

use anyhow::Result;
use colored::Colorize;
use umari_types::{
    ActivateRequest, ListModulesResponse, ModuleDetailsResponse, VersionDetailsResponse,
};

use crate::{client::ApiClient, output};

pub fn upload(
    client: &ApiClient,
    name: String,
    version: String,
    file: PathBuf,
    activate: bool,
) -> Result<()> {
    let response = client.upload_wasm("projectors", &name, &version, &file, activate)?;

    println!(
        "{} uploaded {} v{}",
        "✓".green(),
        response.name,
        response.version
    );
    println!("  sha256: {}", response.sha256);
    println!(
        "  activated: {}",
        if response.activated { "yes" } else { "no" }
    );

    Ok(())
}

pub fn list(client: &ApiClient, active_only: bool, name_filter: Option<String>) -> Result<()> {
    let mut path = String::from("/projectors");
    let mut query_parts = Vec::new();

    if active_only {
        query_parts.push("active_only=true".to_string());
    }
    if let Some(name) = &name_filter {
        query_parts.push(format!("name={name}"));
    }

    if !query_parts.is_empty() {
        path.push('?');
        path.push_str(&query_parts.join("&"));
    }

    let response: ListModulesResponse = client.get(&path)?;
    output::print_modules_table(&response.modules);

    Ok(())
}

pub fn show(client: &ApiClient, name: String, version: Option<String>) -> Result<()> {
    if let Some(ver) = version {
        // Show specific version
        let path = format!("/projectors/{name}/versions/{ver}");
        let response: VersionDetailsResponse = client.get(&path)?;
        output::print_version_details(&response);
    } else {
        // Show all versions
        let path = format!("/projectors/{name}");
        let response: ModuleDetailsResponse = client.get(&path)?;
        output::print_module_details(&response);
    }

    Ok(())
}

pub fn activate(client: &ApiClient, name: String, version: String) -> Result<()> {
    let path = format!("/projectors/{name}/active");
    let body = ActivateRequest {
        version: version.clone(),
    };
    let response: umari_types::ActivateResponse = client.put(&path, &body)?;

    println!("{} activated {name} v{version}", "✓".green());
    if let Some(prev) = response.previous_version {
        println!("  previous version: v{prev}");
    }

    Ok(())
}

pub fn deactivate(client: &ApiClient, name: String) -> Result<()> {
    let path = format!("/projectors/{name}/active");
    let response: umari_types::DeactivateResponse = client.delete(&path)?;

    println!("{} deactivated {name}", "✓".green());
    if let Some(prev) = response.previous_version {
        println!("  version was: {prev}");
    }

    Ok(())
}
