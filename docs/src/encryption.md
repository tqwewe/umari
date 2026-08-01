# Encryption & Crypto-Shredding

Umari provides transparent encryption for sensitive event data. Encryption is AES-256-GCM with per-scope keys, and supports **crypto-shredding**: the irreversible deletion of encryption keys, making all associated event data permanently unreadable.

## How it works

### Marking data for encryption

An event declares an encryption scope: a `prefix:value` string that selects which key encrypts it.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Add `#[crypto_scope]` to the field whose value identifies the scope:

```rust,noplayground
#[derive(Clone, Debug, Event, Serialize, Deserialize)]
#[event_type("task.created")]
pub struct TaskCreated {
    #[domain_id]
    pub user_id: u64,
    #[domain_id]
    pub project_id: Uuid,
    #[crypto_scope]
    pub customer_id: u64,
    pub customer_name: String,
}
```

The scope is formed as `field_name:value`, so this yields `customer_id:12345`. The field does not need to be a domain ID.

{{#endtab }}
{{#tab name="TypeScript" }}

Pass a `cryptoScope` callback to `defineEvent` that returns the scope string:

```ts
import { defineEvent } from "@umari/js";

interface TaskCreatedData {
  userId: number;
  projectId: string;
  customerId: number;
  customerName: string;
}

export const TaskCreated = defineEvent<TaskCreatedData>()("task.created", {
  domainIds: ["userId", "projectId"],
  cryptoScope: (data) => `customer_id:${data.customerId}`,
});
```

Returning `undefined` (or omitting `cryptoScope`) stores the event in plaintext.

{{#endtab }}
{{#endtabs }}

The whole event payload is encrypted, not just the field the scope came from. Each unique scope value gets its own AES-256 key.

### Writing encrypted events

When a command emits an event that declares an encryption scope:

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
4. If the key has been deleted (crypto-shredded), the event data becomes null: folds skip it, projectors/effects receive null data

Encryption/decryption is transparent to your module code. You work with plain typed values (Rust structs or TypeScript objects); the runtime handles the crypto.

### Crypto-shredding

To permanently delete all encrypted data for a scope, delete the key from within an effect:

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
use umari::prelude::delete_crypto_key;

delete_crypto_key("user_id:42")?;
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
import { deleteCryptoKey } from "@umari/js";

deleteCryptoKey("user_id:42");
```

{{#endtab }}
{{#endtabs }}

Or via the API:

```
DELETE /crypto-keys/{scope}
```

This removes the key material from the module store. All events encrypted with that key become permanently unreadable: their data reads as null. This is irreversible by design.

### Key rotation

Key rotation happens automatically. When a new event is written for a scope that already has a key, the existing key is reused. The scope is tied to the domain ID value: `user_id:42` always uses the same key.

If you want to rotate a key (e.g., after a security incident):

1. Delete the existing key (crypto-shredding old events)
2. The next event written for that scope will generate a new key
3. Old events are permanently unreadable; new events use the new key

## Behavior in folds, projectors, and effects

### Folds

When a crypto-shredded event arrives, it has an encryption scope set but its data is null. The fold skips it, so the fold state never reflects the shredded event.

### Projectors and effects

When a crypto-shredded event arrives (encryption scope set, data null), the runtime skips it: `handle()` is not called. This keeps projectors from inserting null rows and effects from acting on missing data.

## Security considerations

- **Keys are stored in SQLite** on the same filesystem as the runtime. Use filesystem encryption (LUKS, etc.) for defense in depth.
- **Key IDs are stored with events**: the event envelope contains `encryption_key_id`, linking each event to its specific key. This allows key rotation without breaking old events.
- **Nonce is derived from event ID**: AES-256-GCM uses the first 12 bytes of the event UUID as the nonce. Event IDs are UUIDv5 values derived from the command's random per-execution correlation and causation IDs, so each event within a scope gets a distinct nonce and reuse is cryptographically negligible.
- **Deletion is instant**: deleting a key immediately makes events unreadable. There's no background re-encryption process.
- **Crypto-shredding is permanent**: deleted keys cannot be recovered. Make sure you've exported data or verified compliance requirements before shredding.
