# umari

An event-sourced runtime for building command handlers, effects, policies, and projectors as WebAssembly modules.

## Overview

Umari provides a runtime and SDK for event-sourced applications. Business logic is compiled to WASM modules and loaded dynamically. The runtime handles event persistence, querying, and module lifecycle.

**Module types:**

- **Commands** — read historical events, validate rules, emit new events
- **Effects** — side effects triggered by events (e.g. sending emails, calling APIs)
- **Policies** — react to events and dispatch new commands
- **Projectors** — build read models from events into SQLite databases

## Crates

| Crate | Description |
|---|---|
| `umari-core` | SDK traits and types for writing WASM modules |
| `umari-macros` | Derive macros (`Event`, `EventSet`, `CommandInput`) |
| `umari-runtime` | Wasmtime-based module runner |
| `umari-api` | HTTP API for managing and executing modules |
| `umari-server` | Server binary |
| `umari-cli` | CLI for uploading and managing modules |
| `umari-ui` | Web UI |

## Building

```sh
cargo build --workspace
```
