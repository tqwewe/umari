use anyhow::Result;
use umari_types::{DeleteEnvVarResponse, GetEnvVarsResponse, SetEnvVarRequest, SetEnvVarResponse};

use crate::{client::ApiClient, output};

pub fn list(client: &ApiClient, module_type: &str, name: &str) -> Result<()> {
    let path = format!("/{module_type}/{name}/env");
    let response: GetEnvVarsResponse = client.get(&path)?;
    output::print_env_vars(&response.vars);
    Ok(())
}

pub fn set(
    client: &ApiClient,
    module_type: &str,
    name: &str,
    key: &str,
    value: &str,
) -> Result<()> {
    let path = format!("/{module_type}/{name}/env/{key}");
    let body = SetEnvVarRequest {
        value: value.to_string(),
    };
    let _response: SetEnvVarResponse = client.put(&path, &body)?;
    println!("✓ set {key} on {name}");
    Ok(())
}

pub fn unset(client: &ApiClient, module_type: &str, name: &str, key: &str) -> Result<()> {
    let path = format!("/{module_type}/{name}/env/{key}");
    let response: DeleteEnvVarResponse = client.delete(&path)?;
    println!("✓ unset {key} from {name}");
    if !response.deleted {
        println!("  (was not set)");
    }
    Ok(())
}
