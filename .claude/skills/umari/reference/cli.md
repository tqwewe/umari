# The `umari` CLI

Install: build from `crates/cli` (`cargo install --path crates/cli` from this repo).

Global flags:
- `-u, --url <URL>` — server URL (default `http://localhost:3000`). Env: `UMARI_URL`.
- `--api-key <KEY>` — API key. Env: `UMARI_API_KEY`. Sent as `Authorization: Bearer <KEY>`.

## Scaffolding — `umari new`

```bash
umari new command create-warranty-plan
umari new projector plans
umari new effect notify-merchant
```

Flags:
- `--lang rust` (default) or `--lang js`.

Behavior (Rust):
1. Runs `cargo metadata --no-deps` to find the workspace root.
2. Creates `<plural>/<name>/{Cargo.toml, src/lib.rs}`.
3. Auto-appends `<plural>/<name>` to the workspace's `members` list in the root `Cargo.toml`.
4. Type names derived from kebab-case → PascalCase: `notify-merchant` → `NotifyMerchant`.
5. Fails if the target directory already exists.

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

**Projector `src/lib.rs`** (with `{type_name}` like `Plans`):
```rust
use umari::prelude::*;

export_projector!(Plans);

#[derive(EventSet)]
enum Query {
    // TODO: add event variants, e.g.: MyEvent(MyEvent),
}

struct Plans {}

impl Projector for Plans {
    type Query = Query;
    fn init() -> anyhow::Result<Self> {
        // TODO: run CREATE TABLE IF NOT EXISTS statements here
        Ok(Plans {})
    }
    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        match event.data {}
    }
}
```

**Effect `src/lib.rs`**:
```rust
use umari::prelude::*;

export_effect!(NotifyMerchant);

#[derive(EventSet)]
enum Query {
    // TODO: add event variants, e.g.: MyEvent(MyEvent),
}

struct NotifyMerchant {}

impl Effect for NotifyMerchant {
    type Query = Query;
    fn init() -> anyhow::Result<Self> {
        Ok(NotifyMerchant {})
    }
    fn partition_key(&self, _event: StoredEvent<Query>) -> Option<String> {
        None
    }
    fn handle(&mut self, event: StoredEvent<Query>) -> anyhow::Result<()> {
        Ok(())
    }
}
```

JS templates also exist; see `--lang js`.

## Building — `umari build`

```bash
umari build                       # all modules in workspace
umari build commands/connect-shop # specific paths
umari build --debug               # debug profile (default is release)
umari build -j 4                  # cap parallel builds (0 = auto)
```

Builds each module to `target/wasm32-wasip2/{release,debug}/<name>.wasm`. Equivalent to `cargo build --target wasm32-wasip2 --release -p <name>` per module.

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
umari commands list --name connect-shop      # filter by name

umari commands show connect-shop             # show details (active version)
umari commands show connect-shop 1.0.5       # specific version

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
umari execute connect-shop '{"shop_id": 42, "name": "Acme"}'
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
# bump version in commands/connect-shop/Cargo.toml
umari deploy commands/connect-shop          # build + upload + activate
```

**Roll back**:
```bash
umari commands list connect-shop            # see versions
umari commands activate connect-shop 1.0.4  # activate previous
```

**Add an env var to a deployed effect**:
```bash
umari effects env notify-merchant set API_KEY sk-prod-xxx
# (effect is re-activated with the new env)
```

**Replay a single projector**:
```bash
umari projectors replay plans
```

**Bulk build only**:
```bash
umari build                # produces .wasm files under target/wasm32-wasip2/release/
```
