# Idempotency Deep Dive

Idempotency is the property that repeating an operation produces the same result as doing it once. In Umari, idempotency is **not optional**: the runtime may redeliver events, effects may be replayed, and commands may be retried. Every module type handles idempotency differently, and this chapter explains each mechanism in detail.

## Why idempotency matters

Several scenarios cause events to be processed more than once:

- **Network retries**: The HTTP client retries a command execution that already succeeded
- **Effect restarts**: A worker crashes after processing an event but before acknowledging it
- **Event store redelivery**: The DCB subscription may redeliver events at checkpoint boundaries
- **Manual replays**: An operator replays a projector or effect from scratch for schema changes
- **Duplicate webhooks**: External services may send the same webhook multiple times

Without idempotency, any of these would produce duplicate events, double-charge customers, or send duplicate emails.

## Command idempotency

### Built-in: idempotency_key

Commands accept an optional `idempotency_key` in `CommandContext`. When present, the runtime checks the event store for any event in the fold scope that carries the same key.

```rust,noplayground
let context = CommandContext::new()
    .with_idempotency_key(Some(request_id));
```

If a matching event is found, the command's execute closure is never called. The runtime returns an empty `ExecuteOutput` with the position of the duplicate event.

**How it works**: During event replay (the fold application phase), the runtime checks each incoming event's `idempotency_key` against the command's key. On match, it commits the transaction with zero new events and returns early.

**When to use**: For commands called with a client-generated request ID. The client retries with the same ID, and Umari ensures the command only executes once.

### Domain-level: state check in execute closure

Commands can also check fold state to decide whether the operation already occurred:

```rust,noplayground
Command::new(input, context)
    .fold::<EventFold<UserRegistered>>()
    .execute(|input, connected| {
        if connected.exists() {
            return Ok(emit![]);  // Already registered
        }
        Ok(emit![UserRegistered { .. }])
    })
```

This is domain-level idempotency: the command understands its own business rules and can determine that "registering a user that's already registered" is a no-op.

**When to use**: When the idempotency logic depends on business state (e.g., "if the project already has this title, don't create a duplicate").

## Projector idempotency

Projectors are **structurally idempotent**. They don't need explicit idempotency logic because:

1. Events are immutable and processed in order
2. The output is determined entirely by the input events
3. Replaying the same events in the same order always produces the same SQLite state

A projector's `handle()` function should be a pure function from `(current_db_state, event)` to `new_db_state`. As long as SQL operations are deterministic (they should be), replay is safe.

**Best practice**: Use `INSERT OR REPLACE` / `INSERT ... ON CONFLICT ... DO UPDATE` for upserts rather than separate INSERT/UPDATE logic. This makes each event handler idempotent at the row level:

```sql
INSERT INTO projects (project_id, user_id, title)
VALUES (?1, ?2, ?3)
ON CONFLICT(project_id) DO UPDATE SET
    title = excluded.title,
    user_id = excluded.user_id;
```

## Effect idempotency

Effect idempotency is the most complex because effects perform external side effects. The standard pattern is **fold-check → side effect → record**, anchored in the event store.

### The pattern

```
┌────────────┐
│  Trigger    │  UserRegistered event arrives
│  Event      │
└─────┬──────┘
      │
      ▼
┌────────────┐
│ 1. Fold-    │  Use FoldQuery to check if the completion event
│    check    │  already exists in the event store
│             │  If yes → exit (already done)
│             │  If no → continue
└─────┬──────┘
      │
      ▼
┌────────────┐
│ 2. Side     │  Make the HTTP call to register the webhook
│    Effect   │
└─────┬──────┘
      │
      ▼
┌────────────┐
│ 3. Record   │  Execute private command
│             │  Emits "WebhookRegistrationCompleted"
│             │  This anchors the fact in the event store
└────────────┘
```

### Why this works

If the effect crashes at any point:

- **During step 1**: FoldQuery is a read-only transaction against the event store. Nothing to recover.
- **During step 2**: On replay, step 1 checks the event store and finds no completion event. The side effect runs again; make the external API call idempotent (the external service should handle duplicates gracefully).
- **During step 3**: The record command is transactional. If committed, step 1 will find it and exit early. If not committed, steps 1-2 will run again; the side effect must be idempotent at the API level.

### FoldQuery is the primary mechanism

Unlike the older "scheduled event" pattern, the recommended approach uses `FoldQuery` to directly check for the completion event:

```rust,noplayground
let already_done = FoldQuery::new()
    .fold(AlreadyRegisteredFold { user_id, topic, current_event_id })
    .run()?;

if already_done {
    return Ok(());  // Work was already completed
}
```

This is lighter than executing a separate private "schedule" command and avoids polluting the event store with intermediate scheduling events. The completion event itself (recorded in step 3) serves as the idempotency anchor.

### SQLite is NOT the idempotency source

A common mistake is using SQLite to track whether work has been done:

```rust,noplayground
// WRONG: SQLite can be deleted and rebuilt
let done: bool = query_row("SELECT done FROM tracking WHERE id = ?1", ...)?;
if done { return Ok(()); }
```

This fails under replay. SQLite databases are derivable: they can be wiped and rebuilt. The only durable source of truth is the event store.

## Anti-patterns

### Using timestamps for idempotency

```rust,noplayground
// WRONG: clock skew, replay produces different timestamps
if event.timestamp > last_processed_timestamp {
    return Ok(());
}
```

Timestamps are not monotonic and change during replay.

### Using in-memory state

```rust,noplayground
// WRONG: doesn't survive restarts
static mut PROCESSED: HashSet<Uuid> = HashSet::new();
```

### Checking external state

```rust,noplayground
// WRONG: external systems aren't part of the event log
let exists = external_api.check_webhook_exists(topic)?;
if exists { return Ok(()); }
```

External APIs can fail, be rate-limited, or return different results over time. The idempotency check must be based on the event store.

## Testing idempotency

Test that your effects are idempotent:

1. Run the effect once: it should perform the side effect
2. Delete the effect's SQLite database
3. Replay all events
4. The effect should NOT perform the side effect again
5. The effect's SQLite should be identical to before

If step 4 produces a duplicate side effect, your idempotency logic is broken.
