---
name: umari
description: Write event-sourced WASM modules with the Umari Rust SDK — commands, projectors, effects, folds, domain IDs, and the `umari` CLI for scaffolding, building, and deploying. Use when the user wants to write or modify Umari modules, design events/folds/domain IDs, build the fold-check → side effect → record idempotency pattern, scaffold via `umari new`, or use `Command::new`, `#[export_command]`, `export_projector!`, `export_effect!`, `Projector`, `Effect`, `Fold`, `EventSet`, or `DomainIds`. Also use when configuring `wasm32-wasip2` builds, `[package.metadata.umari.env]`, or `Cargo.toml` for a Umari workspace.
---

# Umari skill

Umari is a WASM-native event-sourcing runtime. Business logic compiles to WebAssembly components and runs inside a Wasmtime-based runtime. Three module types: **commands** (write events), **projectors** (build SQLite read models), **effects** (call external systems). Consistency uses **Dynamic Consistency Boundaries (DCB)** — no aggregates, no per-entity streams. Each command/fold declares which events it cares about via `EventSet` + domain IDs, and the runtime forms the consistency boundary on the fly.

The SDK crate is `umari` (`crates/umari` in this repo). Modules build for `wasm32-wasip2` and produce `cdylib` + `rlib`. The CLI is `umari`.

## Mental model — the three module types

| Module | Reads | Writes | Trait/Macro | When |
|---|---|---|---|---|
| **Command** | Events (via folds) | Events | `#[export_command]` on `pub fn execute(input, context) -> anyhow::Result<ExecuteOutput>` | The only writers. Validate input → replay relevant events through folds → emit new events. |
| **Projector** | Events | Own SQLite DB | `impl Projector` + `export_projector!(Type)` | Build read models. Must be deterministic & idempotent — replay deletes the DB and reprocesses from position 0. |
| **Effect** | Events | External world | `impl Effect` + `export_effect!(Type)` | Call HTTP, send emails, call other commands. Must be re-runnable — use the fold-check → side effect → record pattern. |

**Commands are the only writers.** Projectors and effects subscribe but never emit. Effects call commands (which write events) using the private-command pattern.

## The canonical command shape

```rust
use umari::prelude::*;
use serde::{Serialize, Deserialize};
use validator::Validate;

#[derive(DomainIds, Validate, Serialize, Deserialize)]
pub struct Input {
    #[domain_id]
    pub shop_id: u64,
    #[domain_id]
    pub plan_id: Uuid,
    #[validate(length(min = 1, max = 200))]
    pub title: String,
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    input.validate()?;
    Command::new(input, context)
        .fold::<ShopExistsFold>()
        .fold::<WarrantyPlanFold>()
        .execute(|input, (shop_exists, plan)| {
            anyhow::ensure!(shop_exists, "shop does not exist");
            anyhow::ensure!(!plan.exists, "plan already exists");
            Ok(emit![WarrantyPlanCreated {
                plan_id: input.plan_id,
                shop_id: input.shop_id,
                title: input.title,
            }])
        })
}
```

Folds register IN ORDER; the execute closure receives their states as a tuple in that same order. Max 12 folds per command.

## Critical invariants (do not violate)

1. **`#[derive(Event)]` does NOT imply `DomainIds`** — events need both: `#[derive(Event, DomainIds, Serialize, Deserialize)]`.
2. **Fold structs need `DomainIds + FromDomainIds`** — `#[derive(DomainIds, FromDomainIds)]`.
3. **`#[crypto_scope]` must be on a field that is ALSO `#[domain_id]`** — the scope value is `"field_name:value"`. The whole payload is encrypted under that scope's key.
4. **EventSet enum is conventionally named `Query`** — one tuple variant per event type, each wrapping the event struct.
5. **`#[scope(field)]` on EventSet variants narrows the query** — without it, ALL the fold's domain ID bindings filter every event type. Within a projector or effect (no input bindings), only `#[scope(field = "literal")]` is meaningful.
6. **Commands call each other as plain functions**, not via a method on a type. Inside an effect, call `my_command::execute(input, CommandContext::new())?` (or for private commands, the bare function name). `CommandContext::new()` inside an effect handler auto-inherits `correlation_id` + `triggering_event_id` from the current event.
7. **Effects must use the fold-check → side effect → record pattern.** Never anchor idempotency in SQLite, timestamps, or external system state — anchor it in the event store via a completion event.
8. **Projector `init()` runs before every replay** — always use `CREATE TABLE IF NOT EXISTS`. `handle()` is wrapped in an implicit transaction; do NOT issue `BEGIN`/`COMMIT`.
9. **Built-in `params!` and SQLite errors only surface constraint violations** — wrong types, unknown columns, `query_one` returning 0 or 2+ rows all TRAP the module.
10. **Module crates must declare `crate-type = ["cdylib", "rlib"]`** and build for `wasm32-wasip2`.

## How to use this skill

Read the reference files on demand — they contain the full patterns, exact derives, common pitfalls, and verbatim templates. **Do not paraphrase from memory; the references are the source of truth.**

| If the user wants to… | Read |
|---|---|
| Write or modify a **command** | `reference/commands.md` (and `reference/folds.md`) |
| Write or modify a **projector** | `reference/projectors.md` |
| Write or modify an **effect** | `reference/effects.md` (and `reference/idempotency.md`) |
| Design **events** or **domain IDs** | `reference/events.md`, `reference/domain-ids.md` |
| Build a **fold** (custom or pick built-in) | `reference/folds.md` |
| Encrypt events or **crypto-shred** | `reference/crypto.md` |
| Set up a new **workspace** or module crate | `reference/project-structure.md` |
| Use the **`umari` CLI** (new/build/deploy/replay) | `reference/cli.md` |
| Make a command **idempotent** | `reference/idempotency.md` |
| Avoid common mistakes | `reference/pitfalls.md` |

Verbatim file templates (matching `umari new` output) live in `templates/`. Read them before writing a new module crate from scratch.

## Workflow when the user asks for a new module

1. **Identify the module type** — command, projector, or effect? If unclear, ask once.
2. **Check the workspace layout** — does the repo have `commands/`, `projectors/`, `effects/` dirs and a root `Cargo.toml` with `[workspace]`? If yes, prefer `umari new <type> <name>` to scaffold (read `reference/cli.md`). If no, read `reference/project-structure.md` first.
3. **Read the relevant reference file** before writing code. Never write a command without reading `reference/commands.md` first — the builder pattern, fold composition, and emit semantics are easy to get subtly wrong.
4. **Cross-check derives and traits** — every event needs `Event + DomainIds + Serialize + Deserialize`; every fold needs `DomainIds + FromDomainIds`; every EventSet enum needs `EventSet` and conventionally is named `Query`.
5. **Verify the build target** — modules build with `cargo build --target wasm32-wasip2` (or `umari build`). Don't suggest a `cargo run` for module code — it's a `cdylib`, not a binary.

## What this skill does NOT cover

- Authoring the runtime, server, or CLI internals (`crates/api`, `crates/runtime`, `crates/server`, `crates/cli`). Use general Rust knowledge for those.
- UmaDB internals — the event store is treated as opaque. Just know it must be running at `UMARI_EVENT_STORE_URL` (default `http://localhost:50051`).
- The TypeScript SDK in `packages/js`. If the user picks `--lang js` for `umari new`, point them at `packages/js/examples` and the corresponding `umari-js` runtime.
