use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::json;
use umari_types::ExecuteResponse;

use crate::client::ApiClient;

pub fn execute(client: &ApiClient, name: String, input_json: String) -> Result<()> {
    // Parse input JSON
    let input: serde_json::Value =
        serde_json::from_str(&input_json).context("failed to parse input JSON")?;

    // Build command payload
    let payload = json!({
        "input": input,
        "context": null,
    });

    let path = format!("/commands/{name}/execute");
    let response: ExecuteResponse = client.post(&path, &payload)?;

    println!("{} command executed", "✓".green());
    if let Some(pos) = response.position {
        println!("  event store position: {pos}");
    }

    if !response.events.is_empty() {
        println!("\n{}:", "EMITTED EVENTS".bold());
        for event in &response.events {
            println!("  - {} [{}]", event.event_type, event.tags.join(", "));
        }
    }

    Ok(())
}
