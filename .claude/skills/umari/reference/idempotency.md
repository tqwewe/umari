# Idempotency

Umari distinguishes two layers of idempotency. Use the right one for the right job — they're not interchangeable.

> **TypeScript** (`@umari/js`): both layers apply identically. Layer A — pass `idempotencyKey` in the command context (`execute(name, input, { idempotencyKey: event.id })`); the runtime short-circuits on a match. Layer B (domain-level) — return `emit()` when the desired state already holds. In effects, guard the side effect with `foldQuery({ … }).run()` before acting (cross-module `execute` returns `void`, so there's no receipt to inspect). See [`javascript.md`](javascript.md#effects).

## Layer A — per-command `idempotency_key` (caller-supplied)

The HTTP client (or upstream service) generates a UUID per request and includes it on the `CommandContext`:

```rust
let ctx = CommandContext::new()
    .with_idempotency_key(Some(request_uuid));
```

During fold replay, the runtime checks each event in scope. If any event's stored `idempotency_key` matches `ctx.idempotency_key`, the runtime short-circuits:

- The execute closure is NOT called.
- Transaction commits with zero new events.
- `ExecuteOutput.position` reflects the head of the store.

**Use for**: safe HTTP retries from clients that can produce a stable request ID.

**Generation**: the SDK doesn't prescribe how to make the key. Patterns:
- Client-supplied (e.g., from an HTTP request body or header — `Idempotency-Key`).
- Deterministic from `(user_id, action, content_hash)` — useful for idempotent UIs.
- Random UUIDs — only safe if the caller persists and reuses them across retries.

**Scope of the check**: the same event must be in the fold query's scope to be seen. If the command queries `user_id:42, project_id:abc` and a previous command emitted with the same key but for `user_id:99`, the new command won't see it. Match scope to the boundary you care about.

## Layer B — fold-check inside effects

Effects must be re-runnable because the runtime may re-deliver events. The pattern:

1. **Fold-check**: read the event store via a fold (directly with `FoldQuery` or implicitly via a private command's fold).
2. **Side effect**: do the external thing.
3. **Record**: emit a completion event.

The completion event itself is the marker — on replay, step 1 sees it and skips step 2.

See `effects.md` for both implementations of the pattern.

## Layer C — domain-level idempotency inside a command's execute closure

A command can recognise that the request leaves the world unchanged and emit no events:

```rust
.execute(|input, project| {
    if project.exists
        && project.title.as_deref() == Some(&input.title)
        && project.description.as_deref() == Some(&input.description)
    {
        return Ok(emit![]);   // same state already — silent success
    }
    if !project.exists {
        return Ok(emit![ProjectCreated { /* ... */ }]);
    }
    Ok(emit![ProjectUpdated { /* ... */ }])
})
```

This is independent of Layers A and B. Use when "doing the thing again would be a no-op" is detectable from fold state. Layer A is needed when the call itself shouldn't be re-executed even if it WOULD change state (e.g., charge a card again).

## Anti-patterns (do not use any of these)

### SQLite as the source of truth

```rust
// WRONG
fn handle(&mut self, event: ...) -> anyhow::Result<()> {
    if query_row("SELECT 1 FROM notified WHERE project_id = ?", params![ev.project_id]).is_some() {
        return Ok(());
    }
    /* ... side effect ... */
    execute("INSERT INTO notified ...", params![ev.project_id])?;
}
```

SQLite is a cache. Replay deletes it. The fold-check is the right way.

### Timestamps

```rust
// WRONG
if event.timestamp < self.last_processed_at { return Ok(()); }
```

Timestamps aren't monotonic across the cluster, and `event.timestamp` is fixed at the time of emission — useless for "have I processed this yet". The event POSITION is monotonic, but the runtime already tracks watermarks; you don't need to.

### In-memory state

```rust
// WRONG
static mut PROCESSED: HashSet<Uuid> = HashSet::new();
```

WASM modules don't persist state between activations. Restart loses everything.

### External-system state check

```rust
// WRONG
let already_sent = self.client.get("https://user.example.com/api/notifications")
    .send()?
    .contains(project_id);
```

The external API isn't part of your event log; it can be inconsistent, slow, or change semantics. Build the answer locally.

## When Layer A and Layer B overlap

A command called from an effect can have BOTH:
- Layer A: caller-supplied `idempotency_key` on `CommandContext`.
- Layer C: the fold detects "already done" via a `UserNotified` event.

Layer A short-circuits via the runtime before the execute closure runs. Layer C runs inside the execute closure. Both produce empty `ExecuteOutput.events`, both let `has_event::<X>()` return false, both are safe.

The effect doesn't need to care which layer triggered — it checks `receipt.has_event::<UserNotified>()` after the call. If false → already done by some mechanism → skip the side effect.

## Testing idempotency

For each effect:
1. Process the event once. Confirm the side effect happens and the completion event is recorded.
2. Replay the effect (`umari effects replay <name>`). Confirm the side effect does NOT happen again.
3. For each command with Layer A: send the same request twice with the same `idempotency_key`. Confirm the second call returns the same position, zero new events.

For each projector: run, drop the DB (`umari projectors replay <name>`), confirm the final state matches.

## Generating idempotency keys deterministically

The SDK uses `IDEMPOTENCY_NAMESPACE = uuid!("e274f2bc-33c5-589f-8643-f3674d86773f")` internally to derive event IDs from `(correlation_id, causation_id, position)`. You can use the same approach to derive command-level idempotency keys:

```rust
use uuid::Uuid;
const MY_NAMESPACE: Uuid = uuid!("...");
let idempotency_key = Uuid::new_v5(&MY_NAMESPACE, &format!("{user_id}:{action}").into_bytes());
```

This makes the key reproducible from request inputs — same logical request, same key, automatic dedup.
