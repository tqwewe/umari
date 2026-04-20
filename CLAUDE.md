# Durable Execution for Umari Effects

## Goal

Replace the user-land `schedule → side effect → record` pattern with runtime-managed durable execution. Effects become straightforward imperative code: make HTTP calls, execute commands. The runtime handles crash recovery, replay, and idempotency transparently by journaling HTTP calls to UmaDB as technical events.

**Core invariant preserved:** deleting all SQLite databases and replaying all events from the beginning produces identical state and performs no duplicate side effects.

**What users write today:**

```rust
let receipt = ScheduleWebhookRegistration::execute(&input)?;
let was_scheduled = receipt.events.iter().any(|e| {
    e.event_type == ShopWebhooksRegistrationScheduled::EVENT_TYPE
});
if !was_scheduled { return Ok(()); }

for topic in ["orders/paid", "orders/cancelled"] {
    let resp = client.post(&url).send()?;
    if !resp.status().is_success() {
        RecordWebhookRegistrationFailed::execute(&fail_input)?;
        return Ok(());
    }
}
RecordWebhooksRegistered::execute(&success_input)?;
```

**What users write after this change:**

```rust
for topic in ["orders/paid", "orders/cancelled"] {
    let resp = client.post(&url).send()?;  // journaled by runtime
    if !resp.status().is_success() {
        RecordWebhookRegistrationFailed::execute(&fail_input)?;
        return Ok(());
    }
}
RecordWebhooksRegistered::execute(&success_input)?;
```

Schedule commands, guard logic, and receipt inspection all disappear. The runtime handles replay correctness.

---

## Key Concepts

### Invocation

An invocation is identified by `(effect_module_name, triggering_event_id)`. Its lifecycle is tracked by two kinds of events in UmaDB:

- Zero or more `umari.effect.http.completed` events, one per HTTP call made during `handle()`
- Exactly one `umari.effect.invocation.completed` event, written after `handle()` returns successfully

**Invocation ID** is derived deterministically: `invocation_id = hash(effect_module_name, triggering_event_id)`. Stable across replays. Used to tag journal entries and derive idempotency keys.

The per-effect cursor in SQLite caches "highest triggering event with a completion event, per effect." It is rebuildable from UmaDB at any time by scanning completion events.

### Sequence Number

Within a single `handle()` execution, host calls are numbered 0, 1, 2, ... in the order the guest makes them. The sequence number plus invocation ID uniquely identifies a journaled operation. Sequence numbers are stable across replays because:

- Effects are deterministic (clock, random, UUIDs all handled already)
- No mid-invocation version swaps (the runtime finishes any in-progress `handle()` before activating a new module version)

### Technical Events

Two new internal event types are introduced:

- `umari.effect.http.completed` — one per HTTP call, carries request and response for replay
- `umari.effect.invocation.completed` — one per successful `handle()`, marks the invocation as done

Both are written to UmaDB directly by the runtime (not through commands — "commands are the only writers" is a user-land convention, not a wasmtime-level rule). They are tagged for retrieval.

**Why a completion event is necessary:** without it, the runtime cannot distinguish "handle(E) finished" from "handle(E) crashed after the last journaled HTTP call but before returning." This matters because `handle()` can do work after its last HTTP call (emit commands, update effect SQLite), and some effects make zero HTTP calls at all — leaving no trace in UmaDB. On SQLite loss, the runtime would have no way to tell which invocations had finished. The completion event makes "finished" an explicit durable fact.

---

## Technical Event Definitions

### `umari.effect.http.completed`

Written once per HTTP call made by an effect, synchronously before the response is returned to the guest.

```rust
struct HttpCompleted {
    // Identity
    invocation_id: String,       // hash(effect_name, triggering_event_id)
    seq: u32,                    // 0, 1, 2, ... within this invocation
    module_version: String,      // module version when this entry was written

    // Request (stored verbatim for integrity checking and observability)
    method: String,
    url: String,
    request_headers: Vec<(String, String)>,
    request_body: Vec<u8>,
    request_hash: [u8; 32],      // hash of full request bytes — integrity check on replay

    // Idempotency key sent to server (None if guest set its own)
    injected_idempotency_key: Option<String>,

    // Response
    status: u16,
    response_headers: Vec<(String, String)>,
    response_body: Vec<u8>,

    // Observability
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
}
```

### `umari.effect.invocation.completed`

Written once per invocation, after `handle()` returns `Ok`. This is the authoritative signal that the invocation finished. Its presence means the invocation is done and will never be re-run.

```rust
struct InvocationCompleted {
    invocation_id: String,        // hash(effect_name, triggering_event_id)
    effect_name: String,
    triggering_event_id: Uuid,
    module_version: String,       // module version that completed the invocation
    http_call_count: u32,         // number of http.completed entries for this invocation
    completed_at: DateTime<Utc>,
}
```

The write must be synchronous and fsynced before the runtime considers the invocation done and advances the SQLite cursor.

### Tags

Tag `http.completed` and `invocation.completed` events with:

- `invocation_id:<hash>` — for journal lookup on replay
- `effect:<effect_module_name>` — for retention and operational queries
- Any domain ID tags from the triggering event — for operational browsing (not required for runtime logic)

### Where They Live

Events are written to UmaDB, alongside domain events. They must be **distinguishable** from domain events at the storage layer so future retention and encryption policies can target them specifically:

- Event type prefix `umari.effect.*` is sufficient
- No separate storage, no separate event log

### Why Only These Two Event Types

We considered a `requested`/`responded` split for HTTP calls to handle the crash window between sending and journaling. It's unnecessary because:

- Idempotency keys sent to the server handle retry dedup at the HTTP layer
- Network failures before the journal write are reattempted on replay (server dedupes)
- Observability of in-flight requests can live in SQLite (not a durability concern)

We also considered omitting the completion event and deriving completion from the presence of HTTP journal entries alone. This fails for effects that do post-HTTP work (commands, effect SQLite writes) after their last HTTP call, and completely fails for effects that make zero HTTP calls — those leave no trace in UmaDB and the runtime cannot tell if they ran. The completion event is the minimum durable signal that an invocation finished.

One HTTP event per call, one completion event per invocation.

---

## Host Interface Changes

### HTTP Client (`wasi-http` replacement or wrapper)

When the guest makes an HTTP call, the host:

1. Computes `seq` (incremented from the invocation's counter)
2. Computes `request_hash` over the outgoing bytes
3. Checks the replay cache (see below) for an entry at this `seq`:
   - **Cache hit:** verify `request_hash` matches the journaled entry's hash. If match, return the journaled response to the guest. If mismatch, halt with a clear determinism error.
   - **Cache miss:** proceed to live execution.
4. For live execution: if the guest has not set `Idempotency-Key`, inject `Idempotency-Key: hash(invocation_id, seq)`.
5. Send the request.
6. On response (including 4xx/5xx): write `http.completed` to UmaDB synchronously (fsync), **before** returning the response to the guest.
7. On network-level failure (timeout, connection refused, DNS): return the error to the guest without journaling. The guest will typically propagate this, `handle()` fails, the runtime retries with backoff. A fresh attempt replays from seq 0 and retries.

### Env, Clock, Random, UUID

These are already deterministic in the guest by existing Umari design. No changes needed.

- Event-handler modules read "current time" as the triggering event's timestamp.
- Random is deterministic at the WASM level.
- UUIDs are derived deterministically (confirmed).

### Command Execution

Commands executed from effects are **not journaled**. They rely on existing fold-based idempotency: on replay, the command re-runs, fetches folds, sees the prior emit, returns `emit![]`. No duplicate events.

Consequences:

- `Command::execute()` from effects **does not return receipts to the guest.** The runtime may track what was emitted internally, but the guest receives something minimal — `()` or `Result<(), Error>` — not `receipt.events`.
- If an effect needs to react to events emitted by a command it executed, it does so by subscribing to those events in a different effect (or policy), not by inspecting a receipt.
- Effects should derive any IDs they need from the triggering event, not from command output.

This is a breaking API change for existing effect code. Migration guidance below.

---

## The Replay Cache

The replay cache is only populated for invocations the runtime is resuming (i.e., journal entries exist but no completion event). Fresh invocations start with an empty cache.

At the start of each `handle()` call that needs replay, the runtime loads journal entries for the current invocation:

```
query UmaDB for events:
    event_type = "umari.effect.http.completed"
    tags contains "invocation_id:<hash>"
    order by seq ascending
```

Load into an ordered structure keyed by `seq`. This is the replay cache for this invocation.

**Version check:** if the cache is non-empty and the first entry's `module_version` does not equal the currently active module version, discard the entire cache (don't replay stale journals from a different code version). Fresh execution follows; idempotency keys at the HTTP server still dedupe previously-sent requests where the API supports it.

The cache lives in runtime memory for the duration of `handle()`. It is not persisted to SQLite.

---

## Idempotency Key Injection

For each HTTP call, the runtime computes:

```
key = hex(hash(invocation_id || seq.to_le_bytes()))
```

If the guest's request does not already contain an `Idempotency-Key` header, the runtime adds this key. If it does, the guest's value wins — that's a signal the author wants business-level dedup, which is stronger.

This key protects against the crash window between send and journal:

- Request sent at step N
- Server processed, response sent
- Process crashes before journal write
- On replay, no journal entry at this seq, runtime sends again with same key
- Server dedupes, returns cached response (for APIs that support idempotency keys)

APIs that do not support idempotency keys fall back to at-least-once semantics. This is unavoidable at the HTTP layer and documented as a caveat.

---

## Completion Tracking

**Completion is defined by the presence of an `umari.effect.invocation.completed` event in UmaDB.**

After `handle(E)` returns `Ok`, the runtime writes `umari.effect.invocation.completed` to UmaDB synchronously (fsync). Only after that write succeeds does the runtime advance the per-effect cursor in SQLite. The cursor is a cache over the completion events; UmaDB is the source of truth.

An invocation for event E is:

- **Complete** iff a matching `invocation.completed` event exists in UmaDB
- **Incomplete** (crashed during `handle()`) iff journal entries exist but no completion event
- **Never started** iff no journal entries and no completion event exist for E

### The Cursor's Role

The SQLite cursor records "highest event position whose invocation has a matching completion event." It exists purely as a cache to avoid scanning UmaDB on every event dispatch. It is not a durability primitive.

When the runtime is healthy and SQLite is intact: dispatch uses the cursor, advances it after each completion event is written. Normal operation never reads the completion events from UmaDB — they exist as the durable record of completion, but the cursor answers the hot-path question "what's the next event to dispatch?"

### SQLite Loss Recovery

On startup, if the cursor is missing or must be rebuilt:

1. For each effect module, scan UmaDB for all `umari.effect.invocation.completed` events
2. Find the highest triggering event position that has a matching completion event
3. Set the cursor to that position

Then resume dispatch from the event after the cursor:

- If that event has an `invocation.completed` → impossible, cursor would already be past it
- If it has `http.completed` entries but no `invocation.completed` → the previous run crashed mid-invocation. Run `handle()` with the replay cache populated. HTTP calls hit the cache. Post-HTTP work re-executes. On success, write the completion event and advance the cursor.
- If it has no journal entries and no completion → never processed. Run `handle()` fresh.

The "delete all SQLite, replay UmaDB" invariant holds unconditionally. No duplicate HTTP requests are sent for any invocation that previously completed, because completion events cause those invocations to be skipped entirely — the replay doesn't enter `handle()` for them at all.

---

## Crash and Deploy Scenarios

### Scenario 1: Crash on same version

- V1 handles event E, makes HTTP call A, journals `http.completed` at seq 0
- V1 makes HTTP call B, crashes before journaling
- Process restarts. V1 still active. Cursor still at E-1.
- Runtime sees: journal entries for E exist, no completion event. Invocation incomplete.
- Loads replay cache for E: one entry at seq 0 with version V1. Version matches.
- Runs `handle(E)` again. At seq 0 (call A), cache hit → return journaled response without HTTP. At seq 1 (call B), cache miss → live execute. Success.
- Writes completion event for E. Cursor advances past E.

**Result:** call A not re-sent, call B sent once. Correct.

### Scenario 2: Crash followed by deploy

- V1 handles event E, makes HTTP call A, journals at seq 0, crashes before call B
- Operator deploys V2 (a fix for whatever caused the crash)
- V2 activates. Cursor still at E-1.
- Runtime sees: journal entries for E exist, no completion event. Invocation incomplete.
- Loads replay cache for E: one entry with version V1. Version mismatch. Discard cache.
- V2 runs `handle(E)` fresh. At seq 0, live execute with key `hash(invocation_id, 0)`. Server dedupes if it previously processed the matching request (if it supports idempotency keys). Otherwise the request is sent again.
- V2 continues through its (possibly different) logic, writes completion event, cursor advances.

**Result:** V2's view of the world is consistent. At-least-once at the HTTP layer for the specific call(s) made during the crashed V1 run. All other events (completed before the crash) are untouched.

### Scenario 3: Normal deploy with no crash

- V1 successfully handles events 1 through 100. Each has a completion event. Cursor past 100.
- Deploy V2.
- V2 handles event 101 onward.

**Result:** no events re-processed, no requests re-sent. Completed invocations stay completed.

### Scenario 4: SQLite deleted, full replay

- System has processed events 1 through 1000. All have completion events in UmaDB.
- SQLite files deleted.
- Runtime starts. For each effect, scans UmaDB for completion events, sets cursor to highest completed position.
- For events 1 through 1000, completion events exist → cursor reaches 1000 immediately, no invocations re-run.
- Dispatch resumes from event 1001.

**Result:** zero HTTP calls made. Zero commands re-executed. Zero side effects. The "delete SQLite, replay from zero" invariant holds cleanly.

### Scenario 5: SQLite deleted, an invocation was incomplete at shutdown

- Events 1 through 999 completed normally (all have completion events).
- Event 1000 was in flight: `handle(1000)` made one HTTP call, journaled it, then the process was killed before completing.
- SQLite deleted.
- Runtime starts. Cursor rebuilds to 999 (highest event with a completion event).
- Dispatches event 1000. Sees journal entries, no completion event. Loads replay cache.
- Runs `handle(1000)`. First HTTP call hits cache. Any subsequent calls execute live. Post-HTTP work runs. Completion written. Cursor advances.

**Result:** one invocation correctly resumes from where it left off. No duplicate HTTP calls for the journaled portion. Live calls for unjournaled portion get server-side dedup within the server's idempotency window.

### Scenario 6: Crash after HTTP calls but before completion write

- V1 handles event E, makes all its HTTP calls, journals them all
- Effect does post-HTTP work: executes a command, writes to its SQLite
- Process crashes before the runtime writes `invocation.completed`
- Restart. Runtime sees journal entries for E, no completion event. Incomplete.
- Re-runs `handle(E)` with replay cache. All HTTP calls hit the cache. Post-HTTP work runs again — command re-executes (fold-idempotent, emits nothing the second time), SQLite upsert writes run again (idempotent by construction).
- Completion event written. Cursor advances.

**Result:** some wasted work on retry, no external duplicates, no incorrect state.

---

## Determinism Requirements

The effect's `handle()` function must be deterministic given:

- The triggering event (same across runs)
- The replay cache (journaled host call outputs, stable per run within a version)
- Current config values (stable, not journaled)

Already handled in Umari:

- Clock reads return the triggering event's timestamp
- Random is deterministic at the WASM level
- UUIDs are generated deterministically

Potential leak points to audit during implementation:

- `HashMap` iteration order (Rust's default hasher is random-seeded) — recommend `BTreeMap` or `IndexMap` in effect code, document in effect author guide
- Library calls that internally read `/dev/urandom`, env vars, or the system clock — caught by the request-hash integrity check on replay

### Integrity Check on Replay

Each journal entry stores `request_hash` — a hash of the full outgoing request bytes. On replay, when the guest re-issues an HTTP call expected to match the journal:

1. Compute hash of the new outgoing request
2. Compare to journaled `request_hash`
3. If mismatch: halt the invocation with a clear error identifying which field differs

This catches determinism regressions (a new library that adds a tracing header, a code change that alters request body formatting) as loud failures instead of silent wrong-data replays.

---

## Effect Author API Changes

### Removed

- `receipt.events`, `receipt.first_run`, or any receipt inspection from effects. Commands executed from effects return `Result<(), Error>`.
- The `ScheduleX` / `RecordXSucceeded` / `RecordXFailed` command pattern for idempotency purposes. (Authors may still use commands that mark business-level state changes — those are domain events, always fine.)

### Unchanged

- `#[derive(EventSet)]` for effect subscription
- `Effect` trait shape: `init()`, `partition_key()`, `handle()`
- `export_effect!(MyEffect)` macro
- Per-effect SQLite for the effect's own internal state

### Added

- Transparent journaling of HTTP calls (no API surface — it Just Works)
- Transparent idempotency key injection

---

## Failure Modes and Policies

### HTTP call returns 4xx/5xx

Journaled as `http.completed`. The guest receives the response and decides what to do. On replay, the same response is returned from the cache. The guest's branching logic runs identically.

### Network-level failure (timeout, connection refused)

Not journaled. The guest receives an error. `handle()` typically propagates it. The runtime catches the failure, schedules retry with backoff. On retry, the replay cache has no entry for the failed seq, so a fresh attempt is made.

### Journal write fails

The runtime treats this as a fatal error for the invocation. Do not return the response to the guest — we cannot guarantee durability. The invocation fails, the runtime retries with backoff. On retry, the HTTP call is re-sent (idempotency key dedupes).

### Response body exceeds size limit

Hard limit (proposed: 1 MB, configurable). If exceeded, the response is not journaled and the invocation fails with a clear error. Authors handle large responses out-of-band (object storage, streaming to disk, etc.) and journal only references.

### Determinism mismatch on replay

Integrity hash differs from journaled value. Halt invocation with a diagnostic error pointing at the differing field. Operator investigates — usually a code change that introduced non-determinism, or an upstream library doing something unexpected.

---

## Storage Considerations

### Size

Typical effect: 1-10 HTTP calls per invocation, 1-10 KB per call, ~10-100 KB per invocation. Manageable for most workloads.

### Retention

Not needed for correctness. Completed invocations' journal entries are never read again. Can be deleted (or crypto-shredded) after a configurable window to reclaim storage.

Recommended approach: do not implement retention initially. Add it when real usage patterns show pressure.

### GDPR / Encryption

Technical events may contain PII (response bodies from external APIs contain customer data in many cases). They must be treated as user data for compliance purposes:

- Apply the same field-level encryption scheme planned for domain events
- Key deletion crypto-shreds technical events when users exercise right-to-erasure
- Shredded entries become unreadable; since no one reads completed-invocation journals, this is operationally harmless

### Body Size Limit

Hard limit on journaled request/response bodies. Default: 1 MB. Configurable per effect or globally. Exceeding the limit fails the invocation with a clear error.

### Compression

Recommend transparent compression (gzip or zstd) at the storage layer. HTTP bodies compress 5-10×. Benefits domain events too. Orthogonal to this design but worth implementing at the same time.

---

## Domain Events vs Technical Events

**Technical events (`umari.effect.http.completed` and `umari.effect.invocation.completed`) are not part of the domain model.**

- Only the runtime reads them
- Policies, projectors, and other effects must never subscribe to them
- Their format is an implementation detail of the runtime
- Effects still emit domain events via commands for facts the rest of the system cares about (e.g., `shop.webhooks.registered`)

Enforcement: the `EventSet` derive macro should reject (or lint against) subscribing to `umari.effect.*` event types from user-land modules.

---

## Migration

### Phase 1: Build the runtime machinery

1. Define `umari.effect.http.completed` and `umari.effect.invocation.completed` event types in the runtime
2. Implement the journaled HTTP host function
3. Implement synchronous `invocation.completed` writes after `handle()` returns Ok
4. Implement the replay cache (load for incomplete invocations, consult on each host call)
5. Implement cursor rebuild from completion events (for SQLite-loss recovery)
6. Implement request-hash integrity checking
7. Implement idempotency key injection
8. Implement module version tagging on journal entries and version-mismatch discard

### Phase 2: Prove the mechanism on one effect

1. Pick a representative effect — the webhook registration effect is a good candidate (has multiple HTTP calls, currently uses the schedule/record pattern)
2. Rewrite it without the schedule/guard/record bookkeeping, relying on runtime journaling
3. Test happy path, crash-during-invocation, crash-then-deploy, replay-from-zero, SQLite-deletion-with-completed-invocations, SQLite-deletion-with-incomplete-invocation
4. Confirm no duplicate HTTP calls are made in any scenario where the invocation previously completed

### Phase 3: Migrate existing effects

1. For each existing effect using schedule/record:
   - Delete the `Schedule*` command and its supporting rule/fold if only used for this purpose
   - Delete the `Record*Succeeded` / `Record*Failed` commands that existed solely as idempotency markers (keep commands that represent genuine domain facts, e.g., `WebhooksRegistered`)
   - Rewrite the effect body as straightforward imperative code
2. Remove any code in effects that inspects `receipt.events`

### Phase 4: Harden API

1. Change `Command::execute()` return type from `Result<Receipt, Error>` to `Result<(), Error>` when called from an effect context
2. Remove receipt types from effect-facing APIs
3. Add compile-time check (or runtime panic with clear message) against subscribing to `umari.effect.*` events from user modules

### What's kept from the old pattern

Commands that emit meaningful domain events remain — `WebhooksRegistered` is still a fact the rest of the system cares about, still emitted by the effect after successful HTTP calls, still subscribable by policies and projectors. Only the idempotency scaffolding goes away.

---

## Open Questions and Future Work

- **Snapshotting effect state mid-invocation.** An alternative to journaling individual host calls is checkpointing the entire WASM memory at yield points. More powerful (no determinism constraint) but substantially more complex. Not needed initially; journaling is sufficient. Can be added later as a fast-path optimization without breaking the journal format.
- **Retention policies for technical events.** Punt until storage pressure is real. Event type prefix (`umari.effect.*`) makes future retention straightforward.
- **Observability hooks.** The runtime has a natural place to emit metrics (invocation duration, replay hit rate, journal size per invocation). Add when needed.
- **Parallel HTTP calls within one `handle()`.** If the guest uses concurrent requests (`join_all` or similar), the runtime can batch journal writes for all completing calls into a single UmaDB transaction. The invariant is "journal before guest observation," not "journal before anything else." Implement when needed; not required for initial correctness.

---

## Summary Checklist

- [ ] Define `umari.effect.http.completed` event type
- [ ] Define `umari.effect.invocation.completed` event type
- [ ] Implement journaled HTTP host function (send, journal synchronously, return)
- [ ] Write `invocation.completed` synchronously after `handle()` returns Ok, before advancing cursor
- [ ] Implement replay cache (load per-invocation entries when resuming an incomplete invocation)
- [ ] Implement cursor rebuild from `invocation.completed` events when SQLite is lost
- [ ] Implement request-hash integrity check on replay
- [ ] Implement idempotency key injection
- [ ] Tag journal entries with `module_version`; discard replay cache on mismatch
- [ ] Change `Command::execute()` to not return receipts when called from effects
- [ ] Prevent user-land subscription to `umari.effect.*` events
- [ ] Migrate one pilot effect, test all crash/deploy/SQLite-loss scenarios
- [ ] Migrate remaining effects
- [ ] Document the author-facing API in the main architecture doc
