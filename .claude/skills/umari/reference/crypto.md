# Encryption & crypto-shredding

Umari supports per-event encryption with **AES-256-GCM**. Each unique scope value gets its own key, stored separately from the events. Delete the key and the event payload becomes permanently unreadable — this is **crypto-shredding**, used for right-to-be-forgotten and data-retention compliance.

> **TypeScript** (`@umari/js`): instead of `#[crypto_scope]`, pass a `cryptoScope` callback to `defineEvent` returning the scope string — e.g. `cryptoScope: (data) => "user_id:" + data.userId`; returning `undefined` stores plaintext. Crypto-shredding is `deleteCryptoKey(...)`. The scope semantics below are identical across SDKs. See [`javascript.md`](javascript.md#events).

## Activation — `#[crypto_scope]`

Mark exactly one field on the event:

```rust
use umari::prelude::*;
use serde::{Serialize, Deserialize};

#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("customer.registered")]
pub struct CustomerRegistered {
    #[domain_id]
    #[crypto_scope]
    pub customer_id: Uuid,
    pub email: String,
    pub display_name: String,
}
```

Hard rule: **`#[crypto_scope]` MUST sit on a field that is also `#[domain_id]`**. The derive does not enforce this, but the scope value is constructed as `"field_name:value"` — without the matching domain ID, the scope can't be looked up. If unsure, treat it as a compile-by-convention requirement.

Only ONE `#[crypto_scope]` per event.

## What gets encrypted

The **entire JSON payload** of the event — not just the marked field. The envelope (`id`, `position`, `event_type`, `tags`, `timestamp`, `correlation_id`, `causation_id`, `triggering_event_id`, `idempotency_key`, `encryption_scope`, `encryption_key_id`) stays plaintext for routing and querying.

The derive generates `fn encryption_scope(&self) -> Option<String>` returning `Some("customer_id:<uuid>")`. The runtime uses this to look up (or create) the per-scope key.

## Lifecycle

1. **Write**: Command emits event → runtime computes scope → looks up/creates AES-256-GCM key → encrypts payload (nonce = first 12 bytes of event ID) → stores ciphertext + `encryption_key_id` in envelope.
2. **Read**: Runtime sees `encryption_scope` is set → looks up key by `encryption_key_id` → decrypts → transparent to your module code.
3. **Shred**: Caller invokes `delete_crypto_key(scope)` → key file deleted → all events tagged with that scope become unreadable. The events themselves are NOT deleted (append-only); their `data` simply becomes `Value::Null` on read.

## Deleting a key

From inside a module:

```rust
use umari::prelude::delete_crypto_key;

delete_crypto_key("customer_id:<uuid-string>".to_string());
```

Or HTTP: `DELETE /crypto-keys/{scope}`.

Permanent. There is no "undelete" — the key is gone.

## Behavior of crypto-shredded events

After shredding, the event is still in the log:
- `event.encryption_scope = Some("customer_id:<uuid>")`
- `event.encryption_key_id = Some(...)`
- `event.data = Value::Null`

**Folds**: silently skip. `EventSet::from_event` returns `None` for null data → `apply` never called. Fold state will NOT reflect shredded events.

**Projectors**: the runtime skips `handle()` for shredded events. The projector's SQLite cannot rebuild rows for those events on replay.

**Effects**: same — `handle()` is not called.

This means **projector queries returning rows for deleted entities will be missing data after a replay**. If the projector needs to keep a "tombstone" row, materialize it from a non-encrypted event before the encrypted one (e.g., a `CustomerCreated` with just the ID, separate from `CustomerRegistered` with PII).

## Designing for shreddability

Pattern: split sensitive events from skeleton events.

```rust
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("customer.id_assigned")]
pub struct CustomerIdAssigned {
    #[domain_id] pub customer_id: Uuid,
    // no PII, no #[crypto_scope]
}

#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("customer.profile_set")]
pub struct CustomerProfileSet {
    #[domain_id]
    #[crypto_scope]
    pub customer_id: Uuid,
    pub email: String,
    pub display_name: String,
}
```

After shredding `customer_id:<uuid>`:
- `CustomerIdAssigned` survives → projectors can still show "customer existed" rows.
- `CustomerProfileSet` is shredded → PII is gone.

## Key rotation

The runtime reuses one key per scope. To rotate:
1. `delete_crypto_key(scope)` — shreds existing events.
2. The next event for that scope will create a fresh key automatically.

There is no "encrypt new events with a new key while keeping old ones readable" mechanism — rotation is destructive by design.

## Common mistakes

- **Marking only `#[crypto_scope]` without `#[domain_id]`**: the scope value still constructs as `"field_name:value"` but routing/queries won't index the field, so you can't replay by scope. Always pair them.
- **Multiple `#[crypto_scope]` fields**: the derive errors with "crypto_scope defined twice". Pick one.
- **Storing a derived field that mirrors a `#[crypto_scope]` field elsewhere unencrypted**: defeats the purpose. Don't put PII in projector tables — derive a non-identifying handle (numeric ID, opaque token) for joining.
- **Assuming shredded events are gone**: the envelope and tags remain. Compliance audits will see "an event happened for this scope at this time"; only the payload is unreadable. If you need the envelope erased too, you need a different deletion mechanism (not in scope for Umari).
- **Calling `delete_crypto_key` from within a fold's apply**: the runtime is mid-replay; deleting a key in the middle is surprising. Do it from an effect or command.

## Operationally

- Key storage is the runtime's responsibility (file-based by default — see runtime crate).
- Backups of key files are the operator's responsibility. **Lose the key files, lose all encrypted events.**
- The `IDEMPOTENCY_NAMESPACE` UUID in the SDK is unrelated to encryption — it's only for generating event IDs.
