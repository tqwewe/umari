# Umari 🌊

[![Discord](https://img.shields.io/badge/Discord-5868e4?logo=discord&logoColor=white)](https://discord.gg/GMX4DV9fbk)
[![Book](https://img.shields.io/badge/Book-0B0d0e?logo=mdbook)](https://umari.tqwewe.com)
[![Sponsor](https://img.shields.io/badge/sponsor-ffffff?logo=githubsponsors)](https://github.com/sponsors/tqwewe)
[![Crates.io Version](https://img.shields.io/crates/v/umari)](https://crates.io/crates/umari)
[![docs.rs](https://img.shields.io/docsrs/umari)](https://docs.rs/umari)
[![Crates.io Total Downloads](https://img.shields.io/crates/d/umari)](https://crates.io/crates/umari)
[![Crates.io License](https://img.shields.io/crates/l/umari)](https://crates.io/crates/umari)
[![GitHub Contributors](https://img.shields.io/github/contributors-anon/tqwewe/umari)](https://github.com/tqwewe/umari/graphs/contributors)

## Introduction

**Umari** is a WASM-native event sourcing runtime for Rust. You write your business logic — commands, projectors, and effects — as ordinary Rust crates, compile them to WebAssembly, and Umari handles event persistence, querying, module lifecycle, and replay.

Umari ships without aggregates and without per-entity streams. Consistency is enforced through **Dynamic Consistency Boundaries (DCB)**: each command declares exactly which events it cares about, and the runtime forms a boundary on the fly. The result is event-sourced systems that are easier to model, easier to evolve, and easier to scale.

Whether you're building a single service or a large event-driven platform, Umari gives you a clean separation between the **decisions** that produce events, the **read models** that answer queries, and the **side effects** that talk to the outside world.

## Key Features

- **WASM-native modules**: Business logic lives in hot-reloadable WebAssembly components, isolated from the runtime and from each other.
- **No aggregates, no streams**: [DCB](https://dcb.events/) replaces per-aggregate streams with dynamic, query-driven consistency boundaries.
- **Three first-class module types**: Commands (the only writers), Projectors (SQLite read models), and Effects (side effects, HTTP, third-party APIs).
- **Folds**: Derive state on demand by replaying just the events a command cares about — no aggregate snapshots, no caches.
- **Fully replayable**: Delete every read model, replay events from position 0, end up in the same state — without re-running side effects.
- **Built-in idempotency**: Per-command idempotency keys, plus a fold-check → side effect → record pattern for effects.
- **Crypto-shredding**: Per-scope AES-256-GCM encryption — delete the key and the events for that scope become permanently unreadable.
- **Type-safe SDK**: Derive macros (`Event`, `EventSet`, `DomainIds`, `FromDomainIds`, `#[export_command]`) keep events, queries, and folds in sync at compile time.
- **First-class operator tools**: HTTP API, CLI, and a built-in web UI for uploading modules, executing commands, and replaying projectors.

## Why Umari?

Most event-sourcing frameworks ask you to design aggregates up front and live with that decision forever. Umari doesn't. You model your events around real-world entities, tag them with domain IDs, and let each command pull exactly the slice of history it needs to make a decision. Consistency is derived from the query, not from a pre-committed grouping.

Compiling business logic to WASM means modules can be uploaded, swapped, and rolled back independently of the runtime — without recompiling or restarting the server. Each module is isolated: a panic in one effect can't take down a projector or the API.

The whole system is designed around a single invariant: **the event store is the source of truth**. Read models are caches. Side effects are derivable. Drop the databases, replay the log, and the system reconverges.

## Use Cases

- **Domain-rich backends**: Order processing, billing, tasks, claims, inventory — anywhere causal history and audit trails matter.
- **Multi-tenant SaaS**: Domain IDs map naturally to tenants, users, projects, or accounts; each command queries its own slice.
- **Workflow and saga orchestration**: Effects react to events, call commands, and form long-running causal chains automatically.
- **Compliance-sensitive systems**: Full audit trail by construction, plus per-scope crypto-shredding for right-to-be-forgotten requests.
- **Integration backbones**: Effects sync state out to webhooks, third-party APIs, or downstream services without leaking concerns into command logic.

## Getting Started

### Prerequisites

- **Rust** — Umari modules are Rust crates compiled to WebAssembly.
- **wasm32-wasip2 target** — `rustup target add wasm32-wasip2`
- **[UmaDB](https://umadb.io)** — the event store. Must be running before starting the Umari server.

### Add the SDK

Add the `umari` SDK to your module crate's `Cargo.toml`:

```toml
[dependencies]
umari = "0.2"
```

Or via command line:

```bash
cargo add umari
```

### Scaffold a project

The `umari` CLI scaffolds new modules and wires them into your workspace:

```bash
umari new command create-project
umari new projector projects
umari new effect notify-user
```

## Basic Example

### Defining an Event

```rust,ignore
use umari::prelude::*;
use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("project.created")]
pub struct ProjectCreated {
    #[domain_id]
    pub project_id: Uuid,
    #[domain_id]
    pub user_id: u64,
    pub title: String,
    pub price: String,
}
```

### Writing a Command

A command validates input, replays the events it cares about through one or more **folds**, and emits new events:

```rust,ignore
use umari::prelude::*;
use serde::{Serialize, Deserialize};
use validator::Validate;

#[derive(DomainIds, Validate, Serialize, Deserialize)]
pub struct Input {
    #[domain_id]
    pub user_id: u64,
    #[domain_id]
    pub project_id: Uuid,
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    pub price: String,
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    input.validate()?;

    Command::new(input, context)
        .fold::<UserExistsFold>()
        .fold::<ProjectFold>()
        .execute(|input, (user_exists, project)| {
            anyhow::ensure!(user_exists, "user does not exist");
            anyhow::ensure!(!project.exists, "project already exists");

            Ok(emit![ProjectCreated {
                project_id: input.project_id,
                user_id: input.user_id,
                title: input.title,
                price: input.price,
            }])
        })
}
```

### Building a Projector

A projector consumes events and writes to its own SQLite database:

```rust,ignore
use umari::prelude::*;

export_projector!(Projects);

#[derive(EventSet)]
enum Query {
    ProjectCreated(ProjectCreated),
}

struct Projects {}

impl Projector for Projects {
    type Query = Query;

    fn init() -> anyhow::Result<Self> {
        execute_batch(
            "CREATE TABLE IF NOT EXISTS projects (
                project_id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                title TEXT NOT NULL,
                price TEXT NOT NULL
            )",
        )?;
        Ok(Projects {})
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        match event.data {
            Query::ProjectCreated(ev) => {
                execute(
                    "INSERT INTO projects (project_id, user_id, title, price) VALUES (?1, ?2, ?3, ?4)",
                    params![ev.project_id, ev.user_id.to_string(), ev.title, ev.price],
                )?;
            }
        }
        Ok(())
    }
}
```

### Reacting with an Effect

Effects react to events with external work — HTTP calls, third-party APIs, sending emails — and stay idempotent via the **fold-check → side effect → record** pattern:

```rust,ignore
use umari::prelude::*;
use wasi_http_client::Client;

export_effect!(NotifyUser);

#[derive(EventSet)]
enum Query {
    ProjectCreated(ProjectCreated),
}

struct NotifyUser {
    client: Client,
}

impl Effect for NotifyUser {
    type Query = Query;

    fn init() -> anyhow::Result<Self> {
        Ok(Self { client: Client::new() })
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        let Query::ProjectCreated(ev) = event.data;

        // 1. Fold-check: ask a private command whether we already notified.
        let receipt = record_notified(
            RecordNotifiedInput { project_id: ev.project_id },
            CommandContext::new(),
        )?;
        if !receipt.has_event::<UserNotified>() {
            return Ok(()); // already notified on a previous run
        }

        // 2. Side effect: actually call out.
        self.client
            .post("https://example.com/notify")
            .json(&ev)
            .send()?;

        Ok(())
    }
}
```

### Deploying

Build and upload every module in the workspace with a single command:

```bash
umari deploy
```

## How It Fits Together

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

Commands are the only writers. Projectors and effects subscribe to the event stream. Effects can call commands, which write events, which trigger more projectors and effects — forming a fully causal, fully replayable chain.

## Workspace Crates

| Crate | Description |
|-------|-------------|
| [`umari`](crates/umari) | **SDK** — traits, types, derive macros, WASM guest library |
| [`umari-macros`](crates/macros) | Derive macros: `Event`, `EventSet`, `DomainIds`, `FromDomainIds`, `#[export_command]` |
| [`umari-runtime`](crates/runtime) | Wasmtime-based module runner, event dispatch, actor system |
| [`umari-api`](crates/api) | HTTP API server (Axum) — upload modules, execute commands, manage lifecycle |
| [`umari-server`](crates/server) | Server binary — runtime + API + web UI |
| [`umari-cli`](crates/cli) | CLI for scaffolding, building, and deploying modules |
| [`umari-ui`](crates/ui) | Web UI built with HTMX |

## Documentation and Resources

- **[The Umari Book](https://umari.tqwewe.com)**: Comprehensive guide — concepts, patterns, SDK reference, and operations.
- **[API Documentation](https://docs.rs/umari)**: Detailed SDK API reference.
- **[Crate on Crates.io](https://crates.io/crates/umari)**: Latest releases and version information.
- **[Community Discord](https://discord.gg/GMX4DV9fbk)**: Ask questions, share what you're building, and chat with other users.

## Examples

The [`packages/js/examples`](packages/js/examples) directory contains TypeScript examples that exercise the HTTP API end-to-end (registering a user, creating a project, etc.). Rust module examples live throughout [The Umari Book](https://umari.tqwewe.com).

## Contributing

Contributions are welcome! Here are ways you can help:

- **Report issues**: Found a bug or have a feature request? [Open an issue](https://github.com/tqwewe/umari/issues).
- **Improve documentation**: PRs against the book in [`docs/src`](docs/src) are especially appreciated. To build it locally: `cargo install mdbook mdbook-tabs`, then `mdbook-tabs install docs` (once, to drop the tab theme assets) and `mdbook serve docs`.
- **Contribute code**: Pick up an open issue or propose new functionality.
- **Join the discussion**: Talk through ideas on [Discord](https://discord.gg/GMX4DV9fbk).

## Support

[![Sponsor](https://img.shields.io/badge/sponsor-ffffff?logo=githubsponsors)](https://github.com/sponsors/tqwewe)

If Umari is useful to you and you'd like to help fund continued development, please consider [sponsoring me on GitHub](https://github.com/sponsors/tqwewe). It directly funds the time I get to spend on the project.

## License

`umari` is licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE) or <http://www.apache.org/licenses/LICENSE-2.0>).

---

[Introduction](#introduction) | [Key Features](#key-features) | [Why Umari?](#why-umari) | [Use Cases](#use-cases) | [Getting Started](#getting-started) | [Basic Example](#basic-example) | [How It Fits Together](#how-it-fits-together) | [Workspace Crates](#workspace-crates) | [Documentation](#documentation-and-resources) | [Examples](#examples) | [Contributing](#contributing) | [Support](#support) | [License](#license)
