use anyhow::Result;
use umari_types::ActiveModulesResponse;

use crate::{client::ApiClient, output};

pub fn active(
    client: &ApiClient,
    module_type: Option<String>,
) -> Result<()> {
    let mut path = String::from("/modules/active");

    if let Some(typ) = module_type {
        path.push_str(&format!("?module_type={typ}"));
    }

    let response: ActiveModulesResponse = client.get(&path)?;
    output::print_active_modules(&response.modules);

    Ok(())
}
