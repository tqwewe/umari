# umari

An event-sourced runtime for building command handlers, effects, and projectors as WebAssembly modules.

## Overview

Umari provides a runtime and SDK for event-sourced applications. Business logic is compiled to WASM modules and loaded dynamically. The runtime handles event persistence, querying, and module lifecycle.

**Module types:**

- **Commands** — read historical events, validate rules, emit new events
- **Effects** — side effects triggered by events (e.g. sending emails, calling APIs)
- **Projectors** — build read models from events into SQLite databases

## Crates

| Crate | Description |
|---|---|
| `umari` | SDK traits and types for writing WASM modules |
| `umari-macros` | Derive macros (`Event`, `EventSet`, `CommandInput`) |
| `umari-runtime` | Wasmtime-based module runner |
| `umari-api` | HTTP API for managing and executing modules |
| `umari-server` | Server binary |
| `umari-cli` | CLI for uploading and managing modules |
| `umari-ui` | Web UI |

## Requirements

- [umadb](https://umadb.io) — used as the event store. Must be running and accessible before starting the server.

## Building

```sh
cargo build --workspace
```

### cargo-make tasks

Install [cargo-make](https://github.com/sagiegurari/cargo-make) if you don't have it:

```sh
cargo install cargo-make
```

| Task | Description |
|---|---|
| `cargo make build` | Clean and build the workspace |
| `cargo make test` | Clean and run tests |
| `cargo make format` | Format all Rust source files |
| `cargo make update-wit-all` | Update all WIT dependency files |
| `cargo make clean-wit-deps-all` | Remove all WIT dependency directories |
