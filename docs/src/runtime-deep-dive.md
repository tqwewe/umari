# Runtime Deep Dive

This chapter covers the internals of the Umari runtime: the actor system, module lifecycle, worker pools, and event processing. Understanding these helps with debugging, performance tuning, and operational decisions.

## Actor hierarchy

The runtime is built on [kameo](https://github.com/tqwewe/kameo), an actor framework for Rust. Each major component is an actor:

```
RuntimeSupervisor
├── PubSub<ModuleEvent>
│   └── Fan-out: all ModuleSupervisors + CommandActor subscribe
├── ModuleStoreActor
│   └── SQLite database for WASM bytes, metadata, env vars, crypto keys
├── CommandActor
│   └── On-demand WASM compilation + command execution
├── ModuleSupervisor<ProjectorWorld>
│   ├── ModuleActor("projects")         ── sequential event processing
│   └── ModuleActor("users")         ── sequential event processing
└── ModuleSupervisor<EffectWorld>
    ├── ModuleActor("register-webhooks")
    │   ├── WorkerActor (global)
    │   └── WorkerActor (keyed × 8)
    └── ModuleActor("create-project")
        ├── WorkerActor (global)
        └── WorkerActor (keyed × 8)
```

## Module lifecycle

### Upload

A `.wasm` file is uploaded via the API (`POST /commands/{name}/upload`). The bytes are stored in the module store (`umari.sqlite`) with the module name, version (from `Cargo.toml`), and type (command/projector/effect). The module is **not** activated: it just sits in the store.

### Activate

When activated (`POST /commands/{name}/activate` with a version), the runtime:

1. Reads the WASM bytes from the store
2. Compiles the component (cached to `cache/*.cwasm` for fast restarts)
3. Spawns a `ModuleActor` for the module
4. The `ModuleActor` opens a subscription to the event store with a DCB query from the module's `query()` method
5. Events start flowing

If a module is already active, activating a new version triggers a **rolling upgrade**:
- The new ModuleActor is spawned
- The old actor receives a stop signal
- When the old actor stops, the new one starts processing from where the old one left off

### Deactivate

Stops the ModuleActor. The SQLite database is preserved; reactivating will resume from the last position.

### Replay

Deletes the SQLite database and restarts from position 0. All events are reprocessed in order. This is the standard way to fix schema changes in projectors.

## Event processing in detail

### Commands (on-demand)

Commands don't subscribe to events. When a command execution request arrives:

1. `CommandActor` compiles the command WASM (from cache or fresh)
2. Creates a `Store` with `CommandComponentState` (has event store client but no SQLite connection)
3. The command's `execute` function runs:
   - Opens a `Transaction` via the `transaction.new()` WIT import
   - Reads events in batches via `transaction.next_batch()`
   - Applies events to folds (checking idempotency)
   - Calls user's execute closure
   - Commits with `transaction.commit()`, which appends events to the event store atomically with a DCB condition check

The DCB condition check ensures no conflicting events were written between reading and committing. If the condition fails (events were written that overlap the query), the command returns an error; the caller should retry.

### Projectors (sequential)

1. `ModuleActor` opens a persistent event store subscription with `stream = true`
2. Event batches arrive via `stream.next_batch()`
3. For each event in the batch:
   - `CURRENT_EVENT_CONTEXT` is set (correlation_id, triggering_event_id)
   - The projector's `handle()` is called
   - SQLite is committed
   - `last_position` is updated

Projectors run on a single actor thread, with no worker pool. This guarantees sequential processing and makes the SQLite connection simple (no thread-safety concerns).

### Effects (parallel)

1. `ModuleActor` opens a persistent event store subscription
2. Event batches arrive via `stream.next_batch()`
3. For each event:
   - The effect's `partition_key()` is called
   - Based on the key:
     - `None` → route to global worker
     - `Some(key)` → route to `hash(key) % POOL_SIZE` worker
   - Workers run on dedicated OS threads (`.spawn_in_thread()`)
4. Workers call the effect's `handle()`
5. Workers ack completion back to the ModuleActor
6. ModuleActor tracks the watermark, the highest contiguous position that all workers have acknowledged

### Watermark algorithm

The watermark is the key to crash recovery for parallel effects:

```
In-flight positions: {5, 7, 8}
Highest completed: 10
Watermark: 4  (5 is the lowest in-flight, so everything before 5 is done)
```

If a worker processing position 5 crashes:
- Position 5 enters backoff
- The watermark stays at 4
- Events after position 5 are queued but not dispatched until 5 is resolved
- The failed position is retried with exponential backoff (up to a maximum)

This ensures **at-least-once** delivery within a partition key while maintaining strict ordering.

## WASM threading model

The runtime uses **dedicated OS threads** for SQLite connections:

- Each `ModuleActor` (projector) runs on one dedicated thread: SQLite is not `Send`, so the actor is bound to that thread
- Each `WorkerActor` (effect worker) runs on its own dedicated thread
- `kameo`'s `.spawn_in_thread()` ensures all async work stays on that thread
- Debug builds include thread-affinity checks that panic if SQLite is accessed from the wrong thread

## Compile cache

WASM compilation is expensive. The runtime caches compiled components:

```
umari-data/cache/
├── 00da7092...cwasm   # Compiled WASM, keyed by content hash
├── 0a5739b9...cwasm
└── ...
```

The cache key is the SHA-256 of the WASM bytes. On restart, modules load from the cache in milliseconds instead of recompiling.

## Module store schema

The module store (`umari.sqlite`) has this schema:

```sql
-- WASM bytecode and metadata
CREATE TABLE modules (
    id TEXT PRIMARY KEY,          -- "type:name:version" (e.g., "command:create-project:1.0.0")
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    module_type TEXT NOT NULL,    -- "command", "projector", "effect"
    wasm_bytes BLOB NOT NULL,
    active BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TEXT NOT NULL
);

-- Environment variables per module
CREATE TABLE env_vars (
    id TEXT NOT NULL,             -- references modules.id
    key TEXT NOT NULL,
    value TEXT NOT NULL
);

-- AES-256 encryption keys per scope
CREATE TABLE crypto_keys (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL,
    key BLOB NOT NULL,            -- 32-byte AES-256 key
    created_at TEXT NOT NULL
);
```

## Backoff and retry

Effects have automatic retry with exponential backoff (`RETRY_ON_FAILURE = true`). When a worker fails:

- The event position enters backoff state
- First retry: after 1 second
- Second: after 2 seconds
- Third: after 4 seconds
- ...up to a maximum of 32 seconds

If the same position fails repeatedly, the backoff keeps increasing. If a new event at a new position succeeds, the backoff resets, indicating the failure was transient.

Projectors do not have automatic backoff (`RETRY_ON_FAILURE = false`). If a projector fails, it stops. The supervisor logs the error and the projector must be manually replayed or fixed.

## Observability

Module output (stdout/stderr) is captured and available via the API:

```
GET /projectors/{name}/output
GET /effects/{name}/output
```

The runtime logs events at different levels:
- `info`: Module started/stopped, replays
- `debug`: Batch commits, watermark advancement
- `warn`: Handler returning inline partition key (shouldn't happen)
- `error`: Worker failures, compilation errors
