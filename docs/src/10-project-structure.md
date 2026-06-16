# 10. Project Structure

This chapter describes how to organize an Umari project workspace. The structure is opinionated but flexible — and `umari init` scaffolds it for you in either language.

## Workspace layout

A project is a workspace with a shared package of events and folds, plus one package/crate per command, projector, and effect.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

A Cargo workspace whose root crate is also the shared library:

```
my-project/
├── Cargo.toml                 # Workspace root + shared library crate
├── rust-toolchain.toml        # Pins the wasm32-wasip2 target
├── src/                       # Shared library: events, folds
│   ├── lib.rs
│   ├── events/
│   │   ├── mod.rs
│   │   ├── user.rs
│   │   ├── project.rs
│   │   └── task.rs
│   └── folds/
│       └── mod.rs
├── commands/
│   ├── create-project/        # crate: create-project
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── update-project/
│   └── archive-project/
├── projectors/
│   ├── projects/              # crate: projects
│   ├── users/
│   └── tasks/
├── effects/
│   └── register-webhooks/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs         # Effect + export_effect!
│           ├── commands.rs    # Private commands
│           └── events.rs      # Effect-private events
└── .gitignore
```

{{#endtab }}
{{#tab name="TypeScript" }}

An npm-workspaces project with a dedicated `shared` package:

```
my-project/
├── package.json               # npm workspace root + dev tooling
├── tsconfig.json              # Base TS config, extended by each package
├── shared/                    # Shared package: @my-project/shared
│   ├── package.json
│   └── src/
│       ├── index.ts
│       ├── events/
│       │   ├── user.ts
│       │   ├── project.ts
│       │   └── task.ts
│       └── folds/
│           └── index.ts
├── commands/
│   ├── create-project/        # package: create-project
│   │   ├── package.json
│   │   ├── tsconfig.json
│   │   └── src/index.ts
│   ├── update-project/
│   └── archive-project/
├── projectors/
│   ├── projects/              # package: projects
│   ├── users/
│   └── tasks/
├── effects/
│   └── register-webhooks/
│       ├── package.json
│       └── src/
│           ├── index.ts       # Effect + exportEffect
│           ├── record-webhook.ts  # Private command(s)
│           └── events.ts      # Effect-private events
└── .gitignore
```

{{#endtab }}
{{#endtabs }}

## Root manifest

The root ties the workspace together and pins shared tooling.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

The root is both the workspace definition and the shared library crate:

```toml
[package]
name = "my-project"
version = "0.1.0"
edition = "2024"

[dependencies]
anyhow.workspace = true
schemars.workspace = true
serde.workspace = true
umari.workspace = true

[workspace]
resolver = "2"
members = ["."]

[workspace.dependencies]
my-project = { path = "." }
umari = "0.2"
anyhow = "1.0"
schemars = "1.2"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1.22", features = ["serde", "v4", "v5"] }
validator = { version = "0.20", features = ["derive"] }
wasi-http-client = { version = "0.2", features = ["json"] }
```

- The root package (`.`) is a workspace member — it's the shared library.
- `my-project = { path = "." }` lets module crates depend on `my-project.workspace = true`.
- `umari` is the SDK crate from crates.io.
- `umari new` appends each new module's path to `members`.

{{#endtab }}
{{#tab name="TypeScript" }}

The root declares the npm workspaces and the shared build tooling (hoisted to every package):

```json
{
  "name": "my-project",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "workspaces": [
    "shared",
    "commands/*",
    "projectors/*",
    "effects/*"
  ],
  "devDependencies": {
    "@umari/js": "^0.1.0",
    "@bytecodealliance/jco": "^1.24.1",
    "@types/node": "^25.9.3",
    "esbuild": "^0.28.1",
    "typescript": "^6.0.3"
  }
}
```

- `@bytecodealliance/jco` and `esbuild` power `umari-js build` (bundle → componentize → wasm).
- Build tooling lives at the root and is hoisted; modules only declare what they import.
- The `commands/*`, `projectors/*`, `effects/*` globs pick up new modules automatically — run `npm install` from the root after `umari new` to link them.

{{#endtab }}
{{#endtabs }}

## Module manifest

Each module is a minimal package/crate that depends on the shared library and the SDK.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```toml
# commands/create-project/Cargo.toml
[package]
name = "create-project"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
my-project.workspace = true     # shared library
umari.workspace = true          # SDK
anyhow.workspace = true
schemars.workspace = true
serde.workspace = true
```

`crate-type = ["cdylib", "rlib"]` is required — `cdylib` produces the `.wasm`, `rlib` enables Rust-level linking for tests. Effects that make HTTP calls add `wasi-http-client.workspace = true`.

{{#endtab }}
{{#tab name="TypeScript" }}

```json
{
  "name": "create-project",
  "version": "0.1.0",
  "type": "module",
  "umari": { "wasm": "dist/module.wasm" },
  "scripts": {
    "build": "umari-js build src/index.ts --out dist/module.wasm"
  },
  "devDependencies": {
    "@umari/js": "^0.1.0",
    "@my-project/shared": "*"
  }
}
```

The `umari` field points at the built wasm so `umari deploy` can find it. The module depends on `@my-project/shared` (the workspace's shared package — resolved locally via npm workspaces) and `@umari/js`. Commands that validate input also add `zod`. The matching `tsconfig.json` extends the root: `{ "extends": "../../tsconfig.json", "include": ["src"] }`.

{{#endtab }}
{{#endtabs }}

## The shared library

Events and folds live in one place that every module imports.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Group events by entity and re-export them:

```rust
// src/events/mod.rs
pub mod user;
pub mod project;
pub mod task;

pub use user::*;
pub use project::*;
pub use task::*;
```

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

`src/lib.rs` declares the modules: `pub mod events; pub mod folds;`.

{{#endtab }}
{{#tab name="TypeScript" }}

The `@my-project/shared` package re-exports everything from `src/index.ts`:

```ts
// shared/src/index.ts
export * from "./events/user.js";
export * from "./events/project.js";
export * from "./events/task.js";
export * from "./folds/index.js";
```

```ts
// shared/src/events/user.ts
import { defineEvent } from "@umari/js";

export type UserRegisteredData = {
  userId: bigint;
  email: string;
  name: string;
};

export const UserRegistered = defineEvent<UserRegisteredData>()("user.registered", {
  domainIds: ["userId"],
  cryptoScope: (data) => `user_id:${data.userId}`,
});
```

```ts
// shared/src/folds/index.ts
import { defineFold } from "@umari/js";
import { UserRegistered } from "../events/user.js";

export const UserExistsFold = defineFold({
  domainIds: ["userId"] as const,
  events: [UserRegistered],
  initial: () => false,
  apply: () => true, // return the next state, reduce-style
});
```

The shared package's `exports` point directly at `src/index.ts`, so modules import TypeScript source — `umari-js build` bundles it per module via esbuild.

{{#endtab }}
{{#endtabs }}

## Naming conventions

| Item | Convention | Example |
|------|-----------|---------|
| Event payload | PascalCase, past tense | `UserRegistered`, `ProjectCreated` |
| Event type string | `object.verb` dot notation | `"user.registered"`, `"project.created"` |
| Command package | kebab-case, imperative | `create-project`, `update-project` |
| Command input | `Input` (Rust struct) / inferred from schema (TS) | `pub struct Input` / `type Input` |
| Projector package | kebab-case, plural noun | `projects`, `users` |
| Projector | PascalCase, plural | `Projects`, `Users` |
| Effect package | kebab-case, verb phrase | `register-webhooks` |
| Effect | PascalCase | `RegisterWebhooks` |
| Fold | PascalCase noun + `Fold` | `UserExistsFold`, `ProjectStateFold` |
| Event set | Rust enum `Query` / TS `events: [...]` array | `enum Query { ... }` / `events: [UserRegistered]` |

Rust domain-ID fields are `snake_case` (`user_id`); TypeScript ones are `camelCase` (`userId`). The tag name follows the field name — keep them consistent if you mix languages over the same events (see [Chapter 4](./04-events.md#domain-ids)).

## Dependencies between module types

```
shared library / package (events, folds)
    ↑                    ↑                    ↑
    |                    |                    |
 commands/          projectors/           effects/
 (import events,    (import events)       (import events,
  import folds)                             import folds,
                                            may have own
                                            events + commands)
```

- Commands import events and folds from the shared library.
- Projectors import events from the shared library.
- Effects import events and folds from the shared library; they may also define their own events and commands locally.

## Working with modules

The `umari` CLI is the intended way to scaffold, build, and deploy — it understands the workspace layout above (in either language), picks up env config, and handles the `wasm32-wasip2` / componentize build for you.

### `umari init`

Scaffold a new workspace in the current directory (or a given path), much like `cargo init`:

```sh
umari init                  # scaffold in the current directory
umari init my-project       # scaffold into ./my-project
umari init --lang js        # choose the language explicitly
```

If `--lang` is omitted, `umari init` prompts for the language. It generates the root manifest, the shared library/package, a `.gitignore`, and (for Rust) `rust-toolchain.toml`, then initializes a git repository if one doesn't already exist. It's non-destructive — existing files are left untouched.

### `umari new`

Scaffold a new module, wire it into the workspace, and drop in starter code:

```sh
umari new command create-project
umari new projector projects
umari new effect register-webhooks
```

`umari new` infers the language from the workspace it's run in (Cargo vs npm), so you don't repeat `--lang`. For a Rust module it appends the crate to the workspace `members`; for a TypeScript module it wires in the `@<project>/shared` dependency — run `npm install` from the root afterward to link it.

### `umari build`

Build every module in the workspace (Rust → `wasm32-wasip2`, TypeScript → bundled + componentized wasm) in release mode. Pass paths to scope the build:

```sh
umari build                                # build everything
umari build commands/create-project        # a single module
umari build commands/ projectors/projects  # multiple paths
umari build --debug                        # debug profile
```

### `umari deploy`

Build everything (same flags as `umari build`) and upload + activate each module against your configured server:

```sh
umari deploy                       # build + upload + activate all modules
umari deploy --no-activate         # upload but don't activate
umari deploy --bump-patch          # auto-bump patch version on conflict
umari deploy commands/create-project
```

For Rust modules, env vars declared under `[package.metadata.umari.env]` in the module's `Cargo.toml` are sent with the upload. TypeScript modules pass env vars via the lower-level upload commands (`--env KEY=VALUE`).

### Manual / lower-level CLI

To bypass the workspace tooling — one-off uploads, custom flags, or your own scripts — build directly and upload through the typed subcommands:

```sh
# Rust
cargo build --target wasm32-wasip2 --release -p create-project
umari commands upload create-project 0.1.0 \
    target/wasm32-wasip2/release/create_project.wasm \
    --env API_KEY=... --activate

# TypeScript
umari-js build commands/create-project/src/index.ts --out commands/create-project/dist/module.wasm
umari commands upload create-project 0.1.0 \
    commands/create-project/dist/module.wasm --activate
```

For production, always build in release mode — debug builds can be 10× larger and slower.
