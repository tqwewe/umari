# 3. Architecture Overview

This chapter describes the Umari runtime architecture — how the pieces connect, how modules are loaded, and how events flow through the system.

## Crate map

Umari is a Cargo workspace with these crates:

| Crate | Role |
|-------|------|
| `umari` (crates/umari) | **SDK** — traits, types, macros, and the WASM guest library |
| `umari-macros` (crates/macros) | Derive macros: `Event`, `EventSet`, `DomainIds`, `FromDomainIds`, `#[export_command]` |
| `umari-runtime` (crates/runtime) | **Runtime** — Wasmtime-based module runner, event dispatch, actor system |
| `umari-api` (crates/api) | HTTP API server (Axum) — upload modules, execute commands, manage lifecycle |
| `umari-server` (crates/server) | Server binary — starts the runtime + API + optional Web UI |
| `umari-cli` (crates/cli) | CLI tool — upload modules, execute commands, manage the system |
| `umari-ui` (crates/ui) | Web UI built with HTMX |
| `umari-types` (crates/types) | Shared API types |

## Runtime architecture

The runtime is built on the [kameo](https://github.com/tqwewe/kameo) actor framework with Wasmtime as the WASM engine.

```
RuntimeSupervisor
├── PubSub<ModuleEvent>          # Module lifecycle events (upload, activate, deactivate)
├── ModuleStoreActor             # SQLite store for WASM bytes, metadata, crypto keys
├── CommandActor                 # Handles command execution requests
│   └── subscribes to module_pubsub for command modules
├── ModuleSupervisor<ProjectorWorld>
│   ├── ModuleActor (projects)      # One per active projector
│   └── ModuleActor (users)
└── ModuleSupervisor<EffectWorld>
    ├── ModuleActor (register-webhooks)
    │   ├── Worker (global)      # Pool of workers for parallel processing
    │   ├── Worker (keyed, 0)    # 8 keyed workers by default
    │   └── Worker (keyed, 7)
    └── ModuleActor (create-project)
        └── Worker (global)      # Workers run on dedicated OS threads
```

### ModuleSupervisor

Each module type (projector, effect) has a `ModuleSupervisor` that:
- Subscribes to `ModuleEvent` via the pubsub
- On `ModuleActivated`: compiles the WASM, spawns a `ModuleActor`
- On `ModuleDeactivated`: stops the `ModuleActor`
- On `ModuleUpdated` (new version uploaded): spawns new actor, waits for old to stop, swaps

### ModuleActor

Each active module has one `ModuleActor` that:
- Opens a subscription to the event store via DCB
- Receives event batches from the event stream
- For projectors: processes events sequentially inline
- For effects: dispatches events to a worker pool based on `partition_key()`
- Maintains a `last_position` watermark in SQLite for crash recovery

### Worker pool (effects only)

Effects use a worker pool for parallel processing. The pool consists of:
- One **global worker** — handles events with no partition key or `PartitionKey::Inline`
- N **keyed workers** (default: 8) — each handles a hash-consistent subset of partition keys

The `partition_key()` method on an effect determines routing. Returning `None` routes to the global worker (sequential for that effect). Returning `Some(key)` routes to `hash(key) % 8`, enabling parallel processing of independent event streams.

Workers acknowledge completion back to the ModuleActor, which tracks the **watermark** — the highest contiguous position acknowledged. The watermark prevents data loss: if a worker crashes, events above the watermark are replayed.

### CommandActor

Commands are different from projectors/effects — they don't subscribe to event streams. Instead, the `CommandActor` handles on-demand execution:

1. A client (HTTP API, effect, or direct call) sends an execution request
2. The CommandActor compiles (or retrieves from cache) the command's WASM
3. The command runs: queries events via DCB, applies folds, executes user logic, emits events
4. Events are appended to the event store atomically

## WASM runtime

Each module runs as a WASM component using the **component model**. The WIT interfaces define the contract between the guest (your module, in Rust or TypeScript) and the host (the Umari runtime):

| WIT interface | Provided to | Purpose |
|---------------|-------------|---------|
| `command/transaction` | Commands, Effects | Read events, commit new events |
| `common` | All modules | Event types, event query types |
| `sqlite` | Projectors, Effects | SQLite database access |
| `crypto` | Effects | Delete encryption keys (crypto-shredding) |
| `wasi:http` | Effects | Make HTTP requests |

Commands have a simpler interface — they only need the transaction interface. Effects have the richest interface, including HTTP and the SQLite database for their own internal state.

> **Note**: An older `command/executor` interface still exists in the WIT package but is unused — effects today execute commands by calling the command function directly (see [Chapter 9: Effects](./09-effects.md)).

## Event flow

```
1. External trigger (HTTP POST /execute)
2. CommandActor instantiates the already-compiled command component
   (all active modules are compiled once at runtime startup and cached)
3. Command executes:
   a. Builds DCB query from fold domain IDs
   b. Opens transaction, reads events in batches
   c. Applies events to folds, checks idempotency
   d. Calls user's execute closure with fold state
   e. User closure emits events (or returns empty emit)
   f. Events appended to event store atomically
4. Event store notifies subscribers
5. Projector ModuleActor receives event batch
6. Projector handles each event, updates SQLite
7. Effect ModuleActor receives event batch
8. Effect dispatches to worker by partition key
9. Worker handles event, may execute commands or make HTTP calls
```

Input validation is a guest-side convention, not part of the runtime contract: the Rust SDK pairs well with crates like `validator`, and the TypeScript SDK accepts an optional [zod](https://zod.dev) schema on `defineCommand`. Each SDK validates before the user logic runs and surfaces failures as an invalid-input error.

## Data directory layout

```
umari-data/
├── umari.sqlite              # Module store: WASM bytes, metadata, crypto keys
├── projector/
│   ├── projects.sqlite           # Per-projector read model
│   ├── projects.log              # Captured stdout/stderr from the projector
│   ├── users.sqlite
│   └── tasks.sqlite
├── effect/
│   ├── register-webhooks.sqlite  # Per-effect internal state
│   ├── register-webhooks.log     # Captured stdout/stderr from the effect
│   └── create-project.sqlite
└── cache/
    └── *.cwasm                # Compiled WASM component cache
```

Anything a module writes to `stdout` or `stderr` (e.g. `println!`, `eprintln!`, `tracing` logs configured to write to stderr) is captured by the runtime, buffered in memory for inspection via the API/UI, and persisted to the per-module `.log` file shown above. Commands don't run as long-lived actors and don't have their own per-module directory.

## Module store

The module store (`umari.sqlite`) is a SQLite database that holds:
- **WASM bytecode** — raw `.wasm` bytes for each module version
- **Module metadata** — name, version, module type, active status
- **Environment variables** — key-value pairs injected as WASI env vars
- **Crypto keys** — AES-256 keys per encryption scope

When a module is uploaded, the bytes are stored. When activated, the WASM is compiled (cached to `cache/*.cwasm`) and a ModuleActor is spawned. Multiple versions can coexist — only one is "active" at a time.
