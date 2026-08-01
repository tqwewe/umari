# Deployment & Operations

This chapter covers building, deploying, and operating Umari in production.

## Building modules

The `umari` CLI builds every module in the workspace, in either language, and handles the `wasm32-wasip2` (Rust) or componentize (TypeScript) build for you.

### Prerequisites

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```sh
rustup target add wasm32-wasip2
```

{{#endtab }}
{{#tab name="TypeScript" }}

```sh
npm install
```

{{#endtab }}
{{#endtabs }}

### Building

```sh
umari build                          # build every module in the workspace
umari build commands/create-project  # build a single module
umari build --debug                  # debug profile
```

`umari deploy` runs the same build and then uploads and activates each module. See [Project Structure](./project-structure.md) for the full CLI.

### Versioning

Each module has a version that the runtime uses for lifecycle management. Increment it when you make changes; the runtime tracks versions and supports rolling upgrades, where activating a new version stops the old one gracefully.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Set the version in the module's `Cargo.toml`:

```toml
[package]
name = "register-user"
version = "1.0.18"
```

{{#endtab }}
{{#tab name="TypeScript" }}

Set the version in the module's `package.json`:

```json
{
  "name": "register-user",
  "version": "1.0.18"
}
```

{{#endtab }}
{{#endtabs }}

## Starting the server

### Binary

```sh
umari \
  --data-dir ./umari-data \
  --event-store-url http://umadb:50051 \
  --api-addr 0.0.0.0:3000
```

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `UMARI_DATA_DIR` | `./umari-data` | Data directory for SQLite, cache |
| `UMARI_EVENT_STORE_URL` | `http://localhost:50051` | UmaDB gRPC endpoint |
| `UMARI_API_ADDR` | `127.0.0.1:3000` | API server bind address |
| `UMARI_API_KEY` | (none) | API key for authentication |
| `UMARI_LOG` | `umari=info` | Log filter (env_logger format) |
| `UMARI_NO_BANNER` | (unset) | Set to any value to hide startup banner |
| `UMARI_VERBOSE` | (unset) | Set to any value for trace-level logging |
| `UMARI_METRICS_INTERVAL` | `15s` | How often to refresh state-derived metrics; `0` disables the collector. See [Monitoring & Alerting](./monitoring.md) |
| `UMARI_SHUTDOWN_TIMEOUT` | `10s` | Graceful shutdown timeout |

### Security

Set `UMARI_API_KEY` in production. Without it, the API is unauthenticated.

```sh
export UMARI_API_KEY="your-secret-key"
```

The API requires `Authorization: Bearer your-secret-key` on all requests. The Web UI uses cookie-based auth with the same key.

## Deploying modules

Most of the time you'll use `umari deploy`, which builds every module in the workspace, uploads it, and activates it in one step. The commands below are the lower-level operations it's built on. Each module type has its own subcommand group: `commands`, `projectors`, and `effects`.

### Via CLI

```sh
# Upload a version (name, version, then the wasm file)
umari commands upload register-user 1.0.18 ./register_user.wasm

# Upload and activate in one step
umari commands upload register-user 1.0.18 ./register_user.wasm --activate

# Activate a previously uploaded version
umari commands activate register-user 1.0.18

# List active modules
umari modules active

# Execute a command (input is passed with --input)
umari execute register-user --input '{"user_id": 42, "email": "user@example.com"}'
```

### Via API

```sh
# Upload a version (the wasm file goes in a multipart "wasm" field)
curl -X POST "http://localhost:3000/commands/register-user/versions/1.0.18" \
  -H "Authorization: Bearer $UMARI_API_KEY" \
  -F "wasm=@register_user.wasm"

# Activate a version
curl -X PUT http://localhost:3000/commands/register-user/active \
  -H "Authorization: Bearer $UMARI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"version": "1.0.18"}'

# Execute a command (the request body is the input JSON itself)
curl -X POST http://localhost:3000/commands/register-user/execute \
  -H "Authorization: Bearer $UMARI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"user_id": 42, "email": "user@example.com"}'
```

To make a command idempotent, send a client-generated `idempotency-key` header on execute; the runtime skips execution if an event carrying that key already exists. The projector and effect groups use the same routes under `/projectors/{name}` and `/effects/{name}`.

## Environment variables for modules

Modules read environment variables through the SDK (`env::var` in Rust, `env()` in TypeScript; see [Effects](./effects.md)). Set them per-module:

```sh
# Via CLI
umari effects env register-webhooks set WEBHOOK_ADDRESS "https://webhook.example.com"

# Via API (the key is in the path, the value in the body)
curl -X PUT http://localhost:3000/effects/register-webhooks/env/WEBHOOK_ADDRESS \
  -H "Authorization: Bearer $UMARI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"value": "https://webhook.example.com"}'
```

Modules can also ship default env values that apply on upload:

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Declare them in the module's `Cargo.toml`, and `umari deploy` picks them up automatically:

```toml
[package.metadata.umari.env]
WEBHOOK_ADDRESS = "https://webhook.example.com"
```

{{#endtab }}
{{#tab name="TypeScript" }}

TypeScript modules have no manifest field for this; pass `--env KEY=VALUE` when uploading:

```sh
umari effects upload register-webhooks 1.0.0 ./register_webhooks.wasm \
  --env WEBHOOK_ADDRESS=https://webhook.example.com
```

{{#endtab }}
{{#endtabs }}

## Operating

### Checking health

```
GET /commands/active
GET /projectors/active
GET /effects/active
```

Each returns the active modules of that type along with their liveness and, for projectors and effects, their committed `last_position`. For numeric metrics and alerting, see [Monitoring & Alerting](./monitoring.md).

### Replaying

```
POST /projectors/{name}/replay
POST /effects/{name}/replay
```

Deletes the SQLite database and reprocesses all events from the beginning. Use for schema changes or recovery.

### Compaction

The event store (UmaDB) should be compacted periodically to reclaim space from deleted or overwritten events. Refer to the UmaDB documentation for compaction procedures.

## Backups

### What to back up

- **UmaDB event store**: the source of truth. Everything else is derivable.
- **`umari.sqlite`**: module store (WASM bytes, crypto keys, metadata). Without this, you'd need to re-upload all modules.
- **Module source code**: WASM bytes in the store are binary; keep source in git.

### What NOT to back up

- **Projector/effect SQLite databases**: these are derivable from events via replay.
- **`cache/*.cwasm`**: compiled cache, regenerated on restart.

## Production checklist

- [ ] `UMARI_API_KEY` is set
- [ ] UmaDB is running and accessible
- [ ] Modules are built with `--release`
- [ ] Env vars are set for all effects
- [ ] Data directory has sufficient disk space (event store grows indefinitely)
- [ ] Logging is configured (`UMARI_LOG`)
- [ ] Backup strategy is in place for UmaDB and `umari.sqlite`
- [ ] [Monitoring and alerting](./monitoring.md) are set up (metrics scrape, dashboard, alerts)
