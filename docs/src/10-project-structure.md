# 10. Project Structure

This chapter describes how to organize an Umari project workspace. The structure is opinionated but flexible — you can adapt it to your needs.

## Workspace layout

```
my-project/
├── Cargo.toml                 # Workspace root + shared library crate
├── src/                       # Shared library: events, folds
│   ├── lib.rs
│   ├── events/
│   │   ├── mod.rs
│   │   ├── user.rs            # User events
│   │   ├── product.rs         # Product events
│   │   └── order.rs           # Order events
│   ├── folds/
│   │   └── mod.rs             # Fold definitions
│   └── helpers.rs             # Utility functions
├── commands/
│   ├── create-product/        # crate: create-product
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── update-product/
│   └── archive-product/
├── projectors/
│   ├── products/              # crate: products
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── users/
│   └── orders/
├── effects/
│   ├── notify-external-service/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs         # Effect + export_effect!
│   │       ├── commands.rs    # Private commands
│   │       └── events.rs      # Effect-private events
│   └── sync-inventory/
└── .gitignore
```

## Root Cargo.toml

The root is both the workspace definition and the shared library crate:

```toml
[package]
name = "my-project"
version = "0.1.0"
edition = "2024"

[dependencies]
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
umari.workspace = true
uuid.workspace = true
validator.workspace = true

[workspace]
resolver = "2"
members = [
    ".",
    "commands/create-product",
    "commands/update-product",
    "projectors/products",
    "projectors/users",
    "effects/notify-external-service",
    # ... etc
]

[workspace.dependencies]
my-project = { path = "." }
umari = { path = "/path/to/umari/crates/umari" }
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1.22", features = ["v4"] }
validator = { version = "0.20", features = ["derive"] }
wasi-http-client = { version = "0.2.1", features = ["json"] }
```

Key points:
- The root package (`.`) is listed as a workspace member — it's the shared library
- `my-project = { path = "." }` lets command/projector/effect crates depend on `my-project.workspace = true`
- `umari` path points to the SDK crate (not the runtime workspace)
- Each module crate uses `crate-type = ["cdylib", "rlib"]`

## Module crate Cargo.toml

Each module is a minimal crate:

```toml
# commands/create-product/Cargo.toml
[package]
name = "create-product"
version = "1.0.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
my-project.workspace = true     # Shared library
umari.workspace = true           # SDK
anyhow.workspace = true
serde.workspace = true
validator.workspace = true
```

`crate-type = ["cdylib", "rlib"]` is required. `cdylib` produces the `.wasm` file; `rlib` enables Rust-level linking for tests.

For effects with HTTP:

```toml
[dependencies]
wasi-http-client.workspace = true
```

For projectors with additional dependencies:

```toml
[dependencies]
rust_decimal.workspace = true
```

## The shared library

The shared library crate (root `src/`) contains:

### Events (`src/events/`)

```rust
// src/events/mod.rs
pub mod user;
pub mod product;
pub mod order;

pub use user::*;
pub use product::*;
pub use order::*;
```

Each event module groups related events:

```rust
// src/events/user.rs
use umari::prelude::*;

#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("user.registered")]
pub struct UserRegistered {
    #[domain_id]
    #[crypto_scope]
    pub user_id: u64,
    pub email: String,
    pub name: String,
}
```

### Folds (`src/folds/`)

```rust
// src/folds/mod.rs
use umari::prelude::*;
use crate::events::*;

#[derive(DomainIds, FromDomainIds)]
pub struct UserExistsFold {
    #[domain_id]
    pub user_id: u64,
}

impl Fold for UserExistsFold {
    type Events = SingleEvent<UserRegistered>;
    type State = bool;

    fn apply(&self, exists: &mut bool, _event: StoredEvent<UserRegistered>) {
        *exists = true;
    }
}
```

### Library root (`src/lib.rs`)

```rust
// src/lib.rs
pub mod events;
pub mod folds;
pub mod helpers;
```

## Naming conventions

| Item | Convention | Example |
|------|-----------|---------|
| Event struct | PascalCase, past tense | `UserRegistered`, `ProductCreated` |
| Event type string | `object.verb` dot notation | `"user.registered"`, `"product.created"` |
| Command crate | kebab-case, imperative | `create-product`, `update-product` |
| Command function | snake_case | `pub fn execute(...)` |
| Command input struct | Always `Input` | `pub struct Input` |
| Projector crate | kebab-case, plural noun | `products`, `users` |
| Projector struct | PascalCase, plural | `Products`, `Users` |
| Effect crate | kebab-case, verb phrase | `notify-external-service` |
| Effect struct | PascalCase | `NotifyExternalService` |
| Fold struct | PascalCase noun + `Fold` | `UserExistsFold`, `ProductStateFold` |
| Fold state | PascalCase noun + `State` | `ProductState`, `WidgetState` |
| EventSet enum | Always `Query` | `enum Query { ... }` |

## Dependencies between module types

```
shared library (events, folds)
    ↑                    ↑                    ↑
    |                    |                    |
 commands/          projectors/           effects/
 (import events,    (import events)       (import events,
  import folds)                             import folds,
                                            may have own
                                            events + commands)
```

- Commands import events and folds from the shared library
- Projectors import events from the shared library
- Effects import events and folds from the shared library; may define their own events and commands locally

## Working with modules

The `umari` CLI is the intended way to scaffold, build, and deploy modules — it understands the workspace layout above, picks up env vars from `[package.metadata.umari.env]`, and handles the `wasm32-wasip2` build for you.

### `umari new`

Scaffold a new module crate, wire it into the workspace `Cargo.toml`, and drop in starter `lib.rs` content:

```sh
umari new command create-product
umari new projector products
umari new effect notify-external-service
```

The default language is Rust; pass `--lang js` to scaffold a JS module instead.

### `umari build`

Build every module crate in the workspace (Rust → `wasm32-wasip2`, JS → componentized wasm) in release mode. Pass paths to scope the build to a subset of crates:

```sh
umari build                                # build everything
umari build commands/create-product        # build a single crate
umari build commands/ projectors/products  # multiple paths
umari build --debug                        # debug profile
umari build -j 4                           # cap parallelism
```

Outputs land in the usual `target/wasm32-wasip2/{release,debug}/<name>.wasm` paths.

### `umari deploy`

Build everything (same flags as `umari build`) and upload + activate each module against the server configured for your client:

```sh
umari deploy                       # build + upload + activate all modules
umari deploy --no-activate         # upload but don't activate
umari deploy --bump-patch          # auto-bump patch version on conflict
umari deploy commands/create-product
```

Env vars declared under `[package.metadata.umari.env]` in each module's `Cargo.toml` are sent along with the upload.

### Manual cargo / lower-level CLI

If you want to bypass the workspace tooling — e.g. one-off uploads, custom build flags, or wiring this into your own scripts — you can still build with plain cargo and upload through the typed subcommands:

```sh
cargo build --target wasm32-wasip2 --release -p create-product

umari commands upload create-product 1.0.0 \
    target/wasm32-wasip2/release/create_product.wasm \
    --env API_KEY=... --activate
```

For production, always build in release mode — debug builds can be 10× larger and slower.

## Adding a new module by hand

If you don't want to use `umari new`:

1. Create the crate directory with `Cargo.toml` and `src/lib.rs`
2. Add it to the workspace `members` list in the root `Cargo.toml`
3. Implement the trait (`Command` via `#[export_command]`, `Projector`, or `Effect`)
4. `umari build` (or `cargo build --target wasm32-wasip2 --release -p <name>`)
5. `umari deploy` (or upload manually via the lower-level CLI / API)
