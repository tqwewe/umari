# Project structure

A Umari project is a Cargo workspace where:
- The root crate is a shared library of events, folds, and helpers.
- Each command/projector/effect is its own module crate under `commands/`, `projectors/`, `effects/`.
- All module crates build for `wasm32-wasip2` and produce `cdylib` + `rlib`.

## Layout

```
my-project/
├── Cargo.toml                  # workspace root + shared lib
├── src/                        # shared crate: events, folds, helpers
│   ├── lib.rs
│   ├── events/
│   │   ├── mod.rs
│   │   ├── shop.rs
│   │   └── warranty.rs
│   └── folds/
│       └── mod.rs
├── commands/
│   ├── connect-shop/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   └── create-warranty-plan/
│       └── ...
├── projectors/
│   ├── plans/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   └── shops/
│       └── ...
└── effects/
    ├── notify-merchant/
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs           # Effect + export_effect!
    │       ├── commands.rs      # private commands (plain fns)
    │       └── events.rs        # effect-private event types
    └── sync-inventory/
        └── ...
```

The `commands/`, `projectors/`, `effects/` directories are **required** for `umari new` and `umari build`/`deploy` — the CLI walks these prefixes.

## Workspace `Cargo.toml`

```toml
[package]
name = "my-project"
version = "0.1.0"
edition = "2024"

[dependencies]
anyhow.workspace = true
serde.workspace = true
umari.workspace = true
uuid.workspace = true
validator.workspace = true

[workspace]
resolver = "2"
members = [
    ".",
    "commands/connect-shop",
    "commands/create-warranty-plan",
    "projectors/plans",
    "projectors/shops",
    "effects/notify-merchant",
]

[workspace.dependencies]
my-project = { path = "." }
umari = "0.2"
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
schemars = "1.2"
uuid = { version = "1.22", features = ["serde", "v4", "v5"] }
validator = { version = "0.20", features = ["derive"] }
wasi-http-client = { version = "0.2", features = ["json"] }
```

The root `[package]` AND `[workspace]` co-existing means the root crate (`.`) is both a workspace member (the shared library) and the workspace root.

`umari new` (Rust mode) auto-appends new module crates to `workspace.members`.

## Module crate `Cargo.toml`

Mirror of `umari new` output:

```toml
[package]
name = "create-warranty-plan"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
my-project.workspace = true   # only added when workspace root has a package name
anyhow.workspace = true
schemars.workspace = true
serde.workspace = true        # only for commands
umari.workspace = true
```

**Notes:**
- `crate-type = ["cdylib", "rlib"]` is required. cdylib produces the `.wasm`; rlib lets other crates (e.g. effects) link it for the private-command pattern.
- `my-project.workspace = true` is added by `umari new` ONLY when the workspace root has a `[package]` (which gives the shared crate a name to depend on).
- Effects additionally need `wasi-http-client` and any command crates they call.
- Projectors and effects don't need `serde.workspace = true` from the CLI template but commonly add it.

## Shared library — `src/lib.rs`

```rust
pub mod events;
pub mod folds;
```

`src/events/mod.rs`:

```rust
mod shop;
mod warranty;
pub use shop::*;
pub use warranty::*;
```

Reasons to centralize events and folds:
- Module crates share definitions — no duplication.
- Renames/refactors propagate via the workspace.
- A new command can import a fold defined for another command.

There's nothing stopping you from defining an event inside a single command's crate — useful when the event is private to that one command (rare). The convention is shared crate first.

## Naming conventions

| Item | Convention | Example |
|---|---|---|
| Event struct | PascalCase past-tense | `WarrantyPlanCreated` |
| `EVENT_TYPE` string | `object.verb` lowercase, dotted | `"warranty.plan.created"` |
| Command crate | kebab-case imperative | `create-warranty-plan` |
| Command function | `pub fn execute` | (fixed) |
| Command input struct | Always `Input` | (fixed) |
| Projector crate | kebab-case plural noun | `plans`, `shops` |
| Projector struct | PascalCase plural noun | `Plans`, `Shops` |
| Effect crate | kebab-case verb phrase | `notify-merchant` |
| Effect struct | PascalCase noun phrase | `NotifyMerchant` |
| Fold struct | PascalCase + `Fold` | `WarrantyPlanFold` |
| Fold state struct | PascalCase + `State` | `WarrantyPlanState` |
| EventSet enum | Always `Query` | (fixed) |

`umari new` auto-converts the kebab-case name to PascalCase for type names.

## Build target

```bash
rustup target add wasm32-wasip2
```

The runtime expects WASI Preview 2 components. WASI Preview 1 will not work.

The dev tooling (e.g. `cargo test` for unit tests, `cargo clippy`) runs on the host normally — module crates compile to native too because of `rlib`. WASM is only the deploy artifact.

## Devenv / Nix

This repo uses Nix flakes + devenv. The flake provides Rust and the `wasm32-wasip2` target. Users of the SDK in their own projects don't need Nix — `rustup target add wasm32-wasip2` is enough.
