use std::time::Duration;

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use kameo::actor::ActorRef;
use serde_json::Value;
use umari_core::event::StoredEventData;
use umari_runtime::module_store::actor::{GetCryptoKey, GetCryptoKeyById, ModuleStoreActor};
use uuid::Uuid;

/// The payload of a stored event once decryption has been attempted.
pub enum EventData {
    Plain(Value),
    Decrypted(Value),
    /// The event was encrypted but its key has been deleted (crypto-shredded).
    CryptoShredded,
}

impl EventData {
    /// The readable JSON value, or `Null` when the data is unavailable.
    pub fn into_value(self) -> Value {
        match self {
            EventData::Plain(v) | EventData::Decrypted(v) => v,
            EventData::CryptoShredded => Value::Null,
        }
    }
}

/// Resolves an event's payload, decrypting it when an encryption scope is set.
pub async fn decrypt_event_data(
    module_store_ref: &ActorRef<ModuleStoreActor>,
    stored: &StoredEventData<Value>,
    uuid: Option<Uuid>,
) -> EventData {
    let Some(scope) = &stored.encryption_scope else {
        return EventData::Plain(stored.data.clone());
    };

    let key: Option<[u8; 32]> = if let Some(key_id) = stored.encryption_key_id {
        module_store_ref
            .ask(GetCryptoKeyById { id: key_id })
            .reply_timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()
            .flatten()
    } else {
        module_store_ref
            .ask(GetCryptoKey {
                scope: scope.as_str().into(),
            })
            .reply_timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()
            .flatten()
            .map(|(_id, key)| key)
    };

    let Some(key) = key else {
        return EventData::CryptoShredded;
    };

    (|| -> Option<EventData> {
        let hex_str = stored.data.as_str()?;
        let ciphertext = hex::decode(hex_str).ok()?;
        let uuid = uuid?;
        let cipher = Aes256Gcm::new_from_slice(&key).ok()?;
        let nonce = Nonce::try_from(&uuid.as_bytes()[..12]).ok()?;
        let plaintext = cipher.decrypt(&nonce, ciphertext.as_ref()).ok()?;
        let value = serde_json::from_slice(&plaintext).ok()?;
        Some(EventData::Decrypted(value))
    })()
    .unwrap_or(EventData::CryptoShredded)
}
