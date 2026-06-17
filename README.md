# Umari 🌊

[![Discord](https://img.shields.io/badge/Discord-5868e4?logo=discord&logoColor=white)](https://discord.gg/GMX4DV9fbk)
[![Book](https://img.shields.io/badge/Book-0B0d0e?logo=mdbook)](https://umari.tqwewe.com)
[![Sponsor](https://img.shields.io/badge/sponsor-ffffff?logo=githubsponsors)](https://github.com/sponsors/tqwewe)
[![Crates.io Version](https://img.shields.io/crates/v/umari)](https://crates.io/crates/umari)
[![docs.rs](https://img.shields.io/docsrs/umari)](https://docs.rs/umari)
[![Crates.io Total Downloads](https://img.shields.io/crates/d/umari)](https://crates.io/crates/umari)
[![Crates.io License](https://img.shields.io/crates/l/umari)](https://crates.io/crates/umari)
[![GitHub Contributors](https://img.shields.io/github/contributors-anon/tqwewe/umari)](https://github.com/tqwewe/umari/graphs/contributors)

**Umari** is a WASM-native event sourcing runtime. You write your business logic — commands, projectors, and effects — as ordinary Rust or TypeScript modules, compile them to WebAssembly, and Umari handles event persistence, querying, module lifecycle, and replay.

Umari ships without aggregates and without per-entity streams. Consistency is enforced through **Dynamic Consistency Boundaries (DCB)**: each command declares exactly which events it cares about, and the runtime forms a boundary on the fly — consistency is derived from the query, not from a pre-committed grouping. The result is event-sourced systems that are easier to model, evolve, and scale.

## Features

- **WASM-native modules** — business logic runs as hot-reloadable, sandboxed WebAssembly components, isolated from the runtime and from each other.
- **No aggregates, no streams** — [DCB](https://dcb.events/) replaces per-aggregate streams with dynamic, query-driven consistency boundaries.
- **Three module types** — commands (the only writers), projectors (SQLite read models), and effects (HTTP and third-party side effects).
- **Folds** — derive state on demand by replaying only the events a command cares about; no snapshots, no caches.
- **Fully replayable** — drop every read model, replay from position 0, and the system reconverges — without re-running side effects.
- **Built-in idempotency** — per-command idempotency keys plus a fold-check → side-effect → record pattern for effects.
- **Crypto-shredding** — per-scope AES-256-GCM encryption; delete the key and those events become permanently unreadable.
- **Rust and TypeScript SDKs** — both compile to the same WASM contract and interoperate over the same events.

## Architecture

| Module | Reads | Writes | Role |
|--------|-------|--------|------|
| **Command** | Events (via folds) | Events | The only writers. Validate input, replay relevant events, emit new ones. |
| **Projector** | Events | Own SQLite DB | Build read models. Deterministic and replayable from position 0. |
| **Effect** | Events | External world | Call HTTP/APIs and trigger commands. Idempotent via fold-check → act → record. |

```
External trigger (HTTP, webhook, cron)
    │
    ▼
Command ──► emits events ──► Event Store (UmaDB)
                                  │
                ┌─────────────────┴─────────────────┐
                ▼                                   ▼
           Projector                              Effect
           (builds read models                   (side effects:
            in SQLite)                            HTTP, calls commands)
                                                     │
                                                     ▼
                                                  Command
                                                  (private, for
                                                   idempotency)
```

Commands are the only writers; projectors and effects subscribe but never emit. Effects call commands, which write events, which trigger more projectors and effects — a fully causal, fully replayable chain. The event store is the single source of truth: read models are caches, and side effects are derivable. Drop the databases, replay the log, and the system reconverges.

## Installation

### Runtime

The server and CLI ship as prebuilt binaries for macOS and Linux (x86_64 and arm64):

```bash
# CLI — scaffold, build, and deploy modules
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/tqwewe/umari/releases/latest/download/umari-cli-installer.sh | sh

# Server — the runtime host
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/tqwewe/umari/releases/latest/download/umari-server-installer.sh | sh
```

The server requires a running [UmaDB](https://umadb.io) event store.

### SDK

```bash
cargo add umari        # Rust (also run: rustup target add wasm32-wasip2)
npm install @umari/js  # TypeScript
```

## Getting Started

```bash
umari init my-app                  # scaffold a Rust or TypeScript workspace
umari new command create-project   # add a command, projector, or effect
umari new projector projects
umari new effect notify-user
umari deploy                       # build and upload every module
```

See **[The Umari Book](https://umari.tqwewe.com)** for concepts, patterns, and complete examples in both Rust and TypeScript.

## Documentation

- **[The Umari Book](https://umari.tqwewe.com)** — guide, patterns, and SDK reference.
- **[API documentation](https://docs.rs/umari)** — Rust SDK reference on docs.rs.
- **[Discord](https://discord.gg/GMX4DV9fbk)** — ask questions and share what you're building.

## Repository Layout

| Crate | Description |
|-------|-------------|
| [`umari`](crates/umari) | **Rust SDK** — traits, types, derive macros, WASM guest library |
| [`umari-macros`](crates/macros) | Derive macros: `Event`, `EventSet`, `DomainIds`, `FromDomainIds`, `#[export_command]` |
| [`umari-runtime`](crates/runtime) | Wasmtime-based module runner, event dispatch, actor system |
| [`umari-api`](crates/api) | HTTP API server (Axum) — upload modules, execute commands, manage lifecycle |
| [`umari-server`](crates/server) | Server binary — runtime + API + web UI |
| [`umari-cli`](crates/cli) | CLI for scaffolding, building, and deploying modules |
| [`umari-ui`](crates/ui) | Web UI built with HTMX |

The TypeScript SDK (`@umari/js`) lives in [`packages/js`](packages/js).

## Contributing

Contributions are welcome — [open an issue](https://github.com/tqwewe/umari/issues), improve the book in [`docs/src`](docs/src), or pick up existing work. To build the book locally: `cargo install mdbook mdbook-tabs`, then `mdbook serve docs` (the tab theme assets are checked in under `docs/theme/`).

## Support

If Umari is useful to you and you'd like to help fund continued development, please consider [sponsoring on GitHub](https://github.com/sponsors/tqwewe). It directly funds the time spent on the project.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE) or <http://www.apache.org/licenses/LICENSE-2.0>).
