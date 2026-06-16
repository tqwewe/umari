# The `umari` CLI

Install: build from `crates/cli` (`cargo install --path crates/cli` from this repo).

Global flags:
- `-u, --url <URL>` — server URL (default `http://localhost:3000`). Env: `UMARI_URL`.
- `--api-key <KEY>` — API key. Env: `UMARI_API_KEY`. Sent as `Authorization: Bearer <KEY>`.

## Scaffolding a workspace — `umari init`

Create a new Umari workspace in the current directory (or a given path), like `cargo init`:

```bash
umari init                  # scaffold in the current directory
umari init my-project       # scaffold into ./my-project
umari init --lang js        # or --lang rust; prompts if omitted
```

Generates the root manifest, the shared library/package, `.gitignore`, and (Rust) `rust-toolchain.toml`, then `git init`s if needed. Non-destructive — existing files are left untouched.

- **Rust** → Cargo workspace whose root crate is the shared library (`umari = "0.2"`, `members = ["."]`).
- **TypeScript** → npm-workspaces project with a `shared` package (`@<name>/shared`) and root dev tooling (`@umari/js`, jco, esbuild, typescript).

## Scaffolding a module — `umari new`

```bash
umari new command create-project
umari new projector projects
umari new effect register-webhooks
```

Flags:
- `--lang rust` or `--lang js`. If omitted, `umari new` **infers the language from the workspace** (Cargo `[workspace]` → Rust; `package.json` `"workspaces"` → TypeScript), and only prompts when it can't tell.

Behavior (Rust):
1. Runs `cargo metadata --no-deps` to find the workspace root.
2. Creates `<plural>/<name>/{Cargo.toml, src/lib.rs}`.
3. Auto-appends `<plural>/<name>` to the workspace's `members` list in the root `Cargo.toml`.
4. Type names derived from kebab-case → PascalCase: `register-webhooks` → `RegisterWebhooks`.
5. Fails if the target directory already exists.

Behavior (TypeScript):
1. Finds the npm workspace root (nearest `package.json` with `"workspaces"`).
2. Creates `<plural>/<name>/{package.json, tsconfig.json, src/index.ts}`.
3. Wires in the `@<project>/shared` dependency (and `zod` for commands). The `commands/*` etc. globs pick it up — run `npm install` from the root to link it.
4. Build via `umari build` / `umari-js build` (esbuild bundle → jco componentize). See `reference/javascript.md`.

Templates emitted (verbatim from `crates/cli/src/commands/new.rs`):

**Command `src/lib.rs`**:
```rust
use schemars::JsonSchema;
use serde::Deserialize;
use umari::prelude::*;

#[derive(DomainIds, JsonSchema, Deserialize)]
pub struct Input {
    // TODO: add input fields; use #[domain_id] to tag domain ID fields
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    Command::new(input, context).execute(|input| {
        // TODO: implement execute
        Ok(emit![])
    })
}
```

**Projector `src/lib.rs`** (with `{type_name}` like `Projects`):
```rust
use umari::prelude::*;

export_projector!(Projects);

#[derive(EventSet)]
enum Query {
    // TODO: add event variants, e.g.: MyEvent(MyEvent),
}

struct Projects {}

impl Projector for Projects {
    type Query = Query;
    fn init() -> anyhow::Result<Self> {
        // TODO: run CREATE TABLE IF NOT EXISTS statements here
        Ok(Projects {})
    }
    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        match event.data {}
    }
}
```

**Effect `src/lib.rs`**:
```rust
use umari::prelude::*;

export_effect!(NotifyUser);

#[derive(EventSet)]
enum Query {
    // TODO: add event variants, e.g.: MyEvent(MyEvent),
}

struct NotifyUser {}

impl Effect for NotifyUser {
    type Query = Query;
    fn init() -> anyhow::Result<Self> {
        Ok(NotifyUser {})
    }
    fn partition_key(&self, _event: StoredEvent<Query>) -> Option<String> {
        None
    }
    fn handle(&mut self, event: StoredEvent<Query>) -> anyhow::Result<()> {
        Ok(())
    }
}
```

For TypeScript module templates and the full `@umari/js` API, see `reference/javascript.md`.

## Building — `umari build`

```bash
umari build                        # all modules in workspace
umari build commands/create-project # specific paths
umari build --debug                # debug profile (default is release)
umari build -j 4                   # cap parallel builds (0 = auto)
```

Builds each module to wasm. Rust → `target/wasm32-wasip2/{release,debug}/<name>.wasm` (equivalent to `cargo build --target wasm32-wasip2 --release -p <name>` per crate). TypeScript → `<module>/dist/module.wasm` via `umari-js build` (esbuild bundle → jco componentize).

## Deploying — `umari deploy`

```bash
umari deploy                      # build + upload + activate
umari deploy --no-activate        # upload only
umari deploy --bump-patch         # auto-bump patch version if already exists
umari deploy --debug              # debug profile
umari deploy -j 4
```

Picks up env vars from each module's `[package.metadata.umari.env]` and sends them with the upload.

## Module management

Three top-level subcommands (note the plural):

```bash
umari commands ...                # command modules
umari projectors ...              # projector modules
umari effects ...                 # effect modules
```

Common subcommands across all three:

```bash
umari commands list                          # list all modules
umari commands list --active-only            # only active versions
umari commands list --name register-user      # filter by name

umari commands show register-user             # show details (active version)
umari commands show register-user 1.0.5       # specific version

umari commands upload <name> <version> <file.wasm>
    [--env KEY=VALUE ...]
    [--activate]

umari commands activate <name> <version>
umari commands deactivate <name>

umari commands env <name> list
umari commands env <name> set KEY VALUE
umari commands env <name> unset KEY
```

Projectors and effects additionally have:

```bash
umari projectors replay <name>    # delete DB, replay from position 0
umari effects replay <name>       # reset subscription to position 0
```

Replay is the bread-and-butter recovery tool — see `projectors.md` and `effects.md`.

Global module view:

```bash
umari modules active              # all active modules across types
umari modules active --type command
```

## Executing a command

```bash
umari execute register-user '{"user_id": 42, "name": "Acme"}'
```

The JSON is the `Input` struct, serialised. Output is the `ExecuteOutput` JSON.

## Server env vars

The server (`crates/server`, run via `cargo run -p umari-server`) reads:

| Var | Default | Purpose |
|---|---|---|
| `UMARI_DATA_DIR` | `./umari-data` | Where SQLite files, key files, and module storage live |
| `UMARI_EVENT_STORE_URL` | `http://localhost:50051` | UmaDB gRPC endpoint |
| `UMARI_API_ADDR` | `127.0.0.1:3000` | HTTP listen address |
| `UMARI_API_KEY` | (none — auth disabled) | If set, clients must send `Authorization: Bearer <key>` |
| `UMARI_LOG` | `umari=info` | `RUST_LOG`-compatible log filter |

## Recipes

**Roll out a new version**:
```bash
# bump version in commands/create-project/Cargo.toml (or package.json)
umari deploy commands/create-project        # build + upload + activate
```

**Roll back**:
```bash
umari commands list create-project          # see versions
umari commands activate create-project 0.1.4 # activate previous
```

**Add an env var to a deployed effect**:
```bash
umari effects env register-webhooks set API_KEY sk-prod-xxx
# (effect is re-activated with the new env)
```

**Replay a single projector**:
```bash
umari projectors replay projects
```

**Bulk build only**:
```bash
umari build                # produces .wasm files under target/wasm32-wasip2/release/
```
