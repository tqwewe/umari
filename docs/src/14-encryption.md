# 14. Encryption & Crypto-Shredding

Umari provides transparent field-level encryption for sensitive event data. Encryption is AES-256-GCM with per-scope keys, and supports **crypto-shredding** — the irreversible deletion of encryption keys, making all associated event data permanently unreadable.

## How it works

### Marking fields for encryption

Add `#[crypto_scope]` to any field that is also annotated with `#[domain_id]`:

```rust
#[derive(Clone, Debug, Event, Serialize, Deserialize)]
#[event_type("project.sold")]
pub struct TaskCreated {
    #[domain_id]
    pub user_id: u64,
    #[domain_id]
    pub project_id: Uuid,
    #[crypto_scope]
    pub customer_id: u64,      // Encrypted at rest
    pub customer_name: String,  // Not encrypted
}
```

The `#[crypto_scope]` attribute must appear on a `#[domain_id]` field. The domain ID value becomes the encryption scope — e.g., `user_id:42` or `customer_id:12345`. Each unique scope value gets its own AES-256 key.

### Writing encrypted events

When a command emits an event with a `#[crypto_scope]` field:

1. The runtime checks whether a key exists for the scope (e.g., `user_id:42`)
2. If not, a new AES-256-GCM key is generated and stored in the module store
3. The entire event data (the JSON) is encrypted with this key using the event ID as the nonce
4. The encrypted ciphertext (hex-encoded) is stored in the event, along with the key ID

The `encryption_scope` and `encryption_key_id` fields on the `StoredEvent` envelope track which scope and key were used.

### Reading encrypted events

When events are read for folds, projectors, or effects:

1. The runtime checks whether the event has an `encryption_key_id`
2. If so, it looks up the key by ID in the module store
3. If the key exists, the event data is decrypted transparently
4. If the key has been deleted (crypto-shredded), the event data becomes `Value::Null` — folds skip it, projectors/effects receive null data

Encryption/decryption is transparent to your module code. You write and read plain Rust structs — the runtime handles the crypto.

### Crypto-shredding

To permanently delete all encrypted data for a scope, delete the key:

```rust
// From within an effect:
use umari::prelude::delete_crypto_key;

delete_crypto_key("user_id:42")?;
```

Or via the API:

```
DELETE /crypto-keys/{scope}
```

This deletes the key from the module store. All events encrypted with that key become permanently unreadable — their data is `Value::Null`. This is irreversible by design.

### Key rotation

Key rotation happens automatically. When a new event is written for a scope that already has a key, the existing key is reused. The scope is tied to the domain ID value — `user_id:42` always uses the same key.

If you want to rotate a key (e.g., after a security incident):

1. Delete the existing key (crypto-shredding old events)
2. The next event written for that scope will generate a new key
3. Old events are permanently unreadable; new events use the new key

## Behavior in folds, projectors, and effects

### Folds

When a crypto-shredded event arrives:
- `event.encryption_scope` is `Some(...)` and `event.data` is `Value::Null`
- `from_event()` returns `None` for null data
- `apply()` is never called — the fold state doesn't reflect the shredded event

### Projectors and effects

When a crypto-shredded event arrives:
- `event.data` is `Value::Null`
- If the event has `encryption_scope` set and data is null, the runtime skips it — `handle()` is not called
- This prevents projectors from inserting null rows

## Security considerations

- **Keys are stored in SQLite** on the same filesystem as the runtime. Use filesystem encryption (LUKS, etc.) for defense in depth.
- **Key IDs are stored with events** — the event envelope contains `encryption_key_id`, linking each event to its specific key. This allows key rotation without breaking old events.
- **Nonce is derived from event ID** — AES-256-GCM uses the first 12 bytes of the event UUID as the nonce. Event IDs are random (UUIDv5 derived), so nonce reuse is cryptographically negligible.
- **Deletion is instant** — deleting a key immediately makes events unreadable. There's no background re-encryption process.
- **Crypto-shredding is permanent** — deleted keys cannot be recovered. Make sure you've exported data or verified compliance requirements before shredding.
