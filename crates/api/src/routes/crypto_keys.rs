use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use umari_runtime::module_store::actor::DeleteCryptoKey;

use crate::{AppState, error::Error};

/// Delete an encryption key by scope, permanently preventing decryption of events encrypted under it (crypto-shredding).
#[utoipa::path(
    delete,
    path = "/crypto-keys/{scope}",
    tag = "crypto-keys",
    params(
        ("scope" = String, Path, description = "Encryption scope identifier (e.g. `user:abc123`)")
    ),
    responses(
        (status = 204, description = "Key deleted"),
        (status = 404, description = "Key not found or already permanently deleted"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn delete_crypto_key(
    State(state): State<AppState>,
    Path(scope): Path<String>,
) -> Result<StatusCode, Error> {
    state
        .module_store_ref
        .ask(DeleteCryptoKey {
            scope: scope.into(),
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
