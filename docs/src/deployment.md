# Deployment & Operations

This chapter covers building, deploying, and operating Umari in production.

## Building modules

### Prerequisites

```sh
rustup target add wasm32-wasip2
cargo install cargo-make  # Optional, for convenience tasks
```

### Building a single module

```sh
cargo build --target wasm32-wasip2 --release -p register-user
```

The output is at `target/wasm32-wasip2/release/register_user.wasm`.

### Building all modules

```sh
cargo build --target wasm32-wasip2 --release --workspace
```

Or use cargo-make:

```sh
cargo make build
```

### Versioning

Each module crate has a version in its `Cargo.toml`. This version is used by the runtime for module management. Increment versions when you make changes:

```toml
[package]
name = "register-user"
version = "1.0.18"
```

The runtime tracks versions and supports rolling upgrades: activate a new version and the old one stops gracefully.

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

### Security

Set `UMARI_API_KEY` in production. Without it, the API is unauthenticated.

```sh
export UMARI_API_KEY="your-secret-key"
```

The API requires `Authorization: Bearer your-secret-key` on all requests. The Web UI uses cookie-based auth with the same key.

## Deploying modules

### Via CLI

```sh
# Upload a module
umari command upload register-user ./register_user.wasm

# Activate a version
umari command activate register-user 1.0.18

# List active modules
umari module list-active

# Execute a command
umari execute register-user '{"user_id": 42, "email": "example.com", ...}'
```

### Via API

```sh
# Upload
curl -X POST http://localhost:3000/commands/register-user/upload \
  -H "Authorization: Bearer $UMARI_API_KEY" \
  -F "wasm=@register_user.wasm"

# Activate
curl -X POST http://localhost:3000/commands/register-user/activate \
  -H "Authorization: Bearer $UMARI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"version": "1.0.18"}'

# Execute
curl -X POST http://localhost:3000/execute \
  -H "Authorization: Bearer $UMARI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"command": "register-user", "input": {"user_id": 42, ...}}'
```

## Environment variables for modules

Modules can access environment variables via `env::var()`. Set them per-module:

```sh
# Via CLI
umari effect env set register-webhooks WEBHOOK_ADDRESS "https://webhook.example.com"

# Via API
curl -X PUT http://localhost:3000/effects/register-webhooks/env \
  -H "Authorization: Bearer $UMARI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"key": "WEBHOOK_ADDRESS", "value": "https://webhook.example.com"}'
```

The module's `Cargo.toml` can declare default env vars:

```toml
[package.metadata.umari.env]
WEBHOOK_ADDRESS = "https://webhook.example.com"
```

## Operating

### Checking health

```
GET /commands/{name}/health
GET /projectors/{name}/health
GET /effects/{name}/health
```

Returns the current `last_position` for projectors and effects.

### Viewing output

```
GET /projectors/{name}/output
GET /effects/{name}/output
```

Returns stdout/stderr from the module. Useful for debugging effect HTTP errors or projector failures.

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
- [ ] Monitoring is set up for module health endpoints
