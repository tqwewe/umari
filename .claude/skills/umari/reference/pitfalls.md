# Common pitfalls

Mistakes Claude (and humans) tend to make when writing Umari modules. Check this list before declaring a task done.

> **TypeScript** (`@umari/js`) pitfalls:
> - **Fold `apply`** is reduce-style — **return the next state**. For a primitive fold (`initial: () => false`) you MUST return (`apply: () => true`); mutating a primitive does nothing. Object/array state may be mutated in place (return optional).
> - **Imports use the `.js` extension** on `.ts` source (`from "../shared/index.js"`), and packages are ESM (`"type": "module"`).
> - **Domain IDs and `position` are `bigint`** — pass them straight to SQLite (which accepts `bigint`); use `.toString()` for HTTP bodies / template strings.
> - **`execute(name, …)` returns `void`** — no receipt. Decide whether to act with `foldQuery({ … }).run()` first.
> - **Validate command input** with an optional `zod` schema on `input`; for fold-state-dependent checks call `reject(msg)` / `invalidInput(msg)` inside `execute`.
> - **Only effects get `fetch`** — commands and projectors are network-free (jco strips http from their builds).

The Rust pitfalls below have direct TypeScript analogues where noted; see [`javascript.md`](javascript.md).

## Derives & macros

- **Forgetting `DomainIds` on an event** — `#[derive(Event, Serialize, Deserialize)]` is INCOMPLETE. Always: `#[derive(Event, DomainIds, Serialize, Deserialize)]`. The error message is misleading — usually surfaces as a trait bound failure on `Fold::Events`.

- **Forgetting `FromDomainIds` on a fold** — `#[derive(DomainIds)]` alone makes a fold uncallable from `.fold::<T>()`. Always derive both on fold structs.

- **`#[crypto_scope]` not on a `#[domain_id]` field** — semantically broken even if it compiles. The scope is `"field:value"`; without a domain ID, you can't recover it.

- **Two `#[crypto_scope]` attributes on one event** — fails to compile. Pick one.

- **Putting `#[event_type("...")]` on a struct field** instead of the struct itself — the attribute is struct-level.

- **Wrong macro name** — `#[derive(EventSet)]` (uppercase S), not `#[derive(Eventset)]`. `#[export_command]` (attribute, not derive). `export_projector!`/`export_effect!` (macros, take the impl type).

## Commands

- **Calling `input.validate()?` inside the execute closure** — too late. Validate before `Command::new`.

- **Putting state-dependent invariants in `validate()`** — `validate()` only checks the input shape. Invariants like "user must exist" need a fold + `anyhow::ensure!` inside the execute closure.

- **Returning `Ok(emit![])` on an actual rejection** — empty emit means "idempotent no-op, succeed". For an actual rejection use `anyhow::bail!("...")` or `anyhow::ensure!(...)`.

- **Reading from SQLite inside a command** — commands don't have SQLite. The SDK doesn't expose it from a command. Build a fold instead.

- **More than 12 folds in one command** — the type system gives up. If you legitimately need more, refactor or compose.

- **Using `Command::new(input.clone(), context)` with the unmodified input** — `Input` doesn't need to be `Clone`; it's moved in once. Don't add `Clone` just for ergonomics.

## Projectors

- **`INSERT` without `ON CONFLICT`** — replay sends the same events again. Constraint violation traps the projector. Use `INSERT ... ON CONFLICT ... DO UPDATE` or `UPDATE` for idempotent writes.

- **`CREATE TABLE` (without `IF NOT EXISTS`)** — `init()` runs on every startup, including post-restart without replay. Trap.

- **Issuing `BEGIN`/`COMMIT` in `handle()`** — every `handle()` call is already wrapped in a transaction by the runtime. Explicit transaction statements interfere.

- **Reading `chrono::Utc::now()` or `std::time::SystemTime`** — non-deterministic. Replay won't produce the same DB. Use `event.timestamp`.

- **HTTP calls from a projector** — projectors have no HTTP capability and shouldn't if they did. Move it to an effect.

- **`query_one` when 0 results is plausible** — traps. Use `query_row` (returns `Option<Row>`).

- **Reading another projector's DB** — projectors are isolated. The other projector's state isn't visible.

## Effects

- **Side effect before fold-check** — replay double-sends. Order: check → DO → record.

- **Recording before the side effect succeeds** — if the side effect returns `Err`, you've already lied to your own log. Order: check → DO → record.

- **`partition_key` returning `None` for high-volume effects** — single global worker bottlenecks. Use a per-entity key.

- **`partition_key` that breaks required ordering** — if events for the same entity must be processed in order, the key must be stable for that entity. Returning `event.id.to_string()` parallelises EVERYTHING and breaks ordering.

- **Using `CommandContext::default()` instead of `::new()` inside `handle()`** — drops the correlation/causation chain. The triggering event isn't recorded. Always `::new()`.

- **Forgetting to add the called command crate as a dependency** — `use my_command::execute` won't resolve. Update the effect's `Cargo.toml`.

- **`static mut` for idempotency tracking** — doesn't survive restart. Use a fold or completion event.

- **Anchoring idempotency in SQLite** — effects don't have SQLite, and even if they did, replay would wipe it. Anchor in the event store.

## Cargo / build

- **Missing `crate-type = ["cdylib", "rlib"]`** on a module crate — `umari deploy` can't produce a `.wasm`. Lib-only crates won't build as components.

- **Building for `wasm32-wasi`** instead of `wasm32-wasip2` — wrong target. The runtime expects Preview 2 components.

- **Module crate not in `workspace.members`** — `umari build` / `umari deploy` won't see it. `umari new` adds it automatically; manual creation must add it manually.

- **Workspace `Cargo.toml` missing `[package]`** — fine, just means individual module crates won't list `my-project.workspace = true` (no name to depend on). `umari new` handles this gracefully.

## Naming

- **EventSet enum named anything other than `Query`** — works fine technically, but breaks convention. Stick to `Query`.

- **Command function not named `execute`** — works, but unusual. Most patterns assume `execute`. The `#[export_command]` macro generates `{PascalCase(fn_name)}Export` for the ZST, so any name works but `execute` is the convention.

- **`EVENT_TYPE` string in PascalCase** — works, but the convention is `object.verb` lowercase dotted: `project.created`, not `ProjectCreated`.

- **Module crate name in PascalCase** — `cargo` rejects this anyway, but worth knowing — kebab-case only.

## CLI

- **Using singular `umari command upload`** — the subcommand is plural: `umari commands upload`. Same for `projectors` and `effects`.

- **Running `umari new` from outside the workspace** — `cargo metadata` won't find the workspace root. Run from anywhere inside the workspace tree.

## Misc

- **Importing items individually instead of `umari::prelude::*`** — works, but verbose. Default to the prelude unless you have a specific reason.

- **Adding `Clone`/`Debug` to events you don't need to debug/clone** — harmless but noisy.

- **Treating UmaDB position as a stable cross-replay identifier** — position is stable per UmaDB instance, but if you re-create UmaDB from scratch (e.g., dev environment reset), positions shift. Use `event.id` (UUID) for cross-instance references.

- **Storing wallclock timestamps when `event.timestamp` is in scope** — `event.timestamp` is captured at emission and stable across replays. `chrono::Utc::now()` inside a projector/effect is not.
