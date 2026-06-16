# Project structure

A Umari project is a Cargo workspace where:
- The root crate is a shared library of events, folds, and helpers.
- Each command/projector/effect is its own module crate under `commands/`, `projectors/`, `effects/`.
- All module crates build for `wasm32-wasip2` and produce `cdylib` + `rlib`.

> **TypeScript** (`@umari/js`): a project is an **npm-workspaces** project instead — a `shared` package (`@<project>/shared`) for events/folds, plus one package per module under `commands/`/`projectors/`/`effects/`, with build tooling (`@umari/js`, jco, esbuild, typescript) hoisted to the root `package.json`. Imports use the `.js` extension on `.ts` source. `umari init --lang js` scaffolds it; see [`javascript.md`](javascript.md#setup--workspace) and `cli.md`.

## Layout

```
my-project/
├── Cargo.toml                  # workspace root + shared lib
├── src/                        # shared crate: events, folds, helpers
│   ├── lib.rs
│   ├── events/
│   │   ├── mod.rs
│   │   ├── user.rs
│   │   └── task.rs
│   └── folds/
│       └── mod.rs
├── commands/
│   ├── register-user/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   └── create-project/
│       └── ...
├── projectors/
│   ├── projects/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   └── users/
│       └── ...
└── effects/
    ├── notify-user/
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
    "commands/register-user",
    "commands/create-project",
    "projectors/projects",
    "projectors/users",
    "effects/notify-user",
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
name = "create-project"
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
mod user;
mod task;
pub use user::*;
pub use task::*;
```

Reasons to centralize events and folds:
- Module crates share definitions — no duplication.
- Renames/refactors propagate via the workspace.
- A new command can import a fold defined for another command.

There's nothing stopping you from defining an event inside a single command's crate — useful when the event is private to that one command (rare). The convention is shared crate first.

## Naming conventions

| Item | Convention | Example |
|---|---|---|
| Event struct | PascalCase past-tense | `ProjectCreated` |
| `EVENT_TYPE` string | `object.verb` lowercase, dotted | `"project.created"` |
| Command crate | kebab-case imperative | `create-project` |
| Command function | `pub fn execute` | (fixed) |
| Command input struct | Always `Input` | (fixed) |
| Projector crate | kebab-case plural noun | `projects`, `users` |
| Projector struct | PascalCase plural noun | `Projects`, `Users` |
| Effect crate | kebab-case verb phrase | `notify-user` |
| Effect struct | PascalCase noun phrase | `NotifyUser` |
| Fold struct | PascalCase + `Fold` | `ProjectFold` |
| Fold state struct | PascalCase + `State` | `ProjectState` |
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
