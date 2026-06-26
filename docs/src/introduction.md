# Introduction

Umari is a **WASM-native event sourcing runtime**. You write business logic in Rust or TypeScript, compile it to WebAssembly, and the runtime handles event persistence, module lifecycle, and state derivation.

> **Two SDKs, one runtime.** Modules can be authored with the Rust SDK (`umari`) or the TypeScript SDK (`@umari/js`). Both compile to the same WASM component contract and produce interchangeable modules, so a project can even mix languages. Throughout this book, code examples are shown in tabs; pick your language once and every snippet follows.

## What is event sourcing?

In event sourcing, every state change is recorded as an **immutable event** in an append-only log. The current state of the system is derived by replaying events from the beginning. This gives you:

- **Full audit trail**: every change is recorded, forever
- **Temporal queries**: ask "what was the state at time T?" by replaying up to that point
- **Complete replayability**: delete all derived state and rebuild it from events
- **Causal traceability**: every event knows which action produced it and which event triggered that action

Umari adds two key innovations on top of classical event sourcing:

1. **No aggregates, no streams**: consistency boundaries are dynamic (DCB), not pre-partitioned
2. **Everything is WASM**: business logic is compiled to WebAssembly and loaded at runtime

## Three module types

Umari splits business logic into three distinct concerns, each compiled as a separate WASM module:

| Module | Job | Writes events? | SQLite? |
|--------|-----|----------------|---------|
| **Command** | Validate input, check invariants, emit events | Yes, the only writer | No |
| **Projector** | Build queryable read models from the event stream | No | Yes |
| **Effect** | React to events with side effects (HTTP, email, third-party APIs) | Only by calling commands | Yes |

Commands are the **only mechanism for writing events**. Projectors and effects subscribe to events and react. Effects can call commands, which write events, which trigger more projectors and effects, forming a causal chain.

## How it fits together

```
External trigger (HTTP, webhook, cron)
    │
    ▼
Command ──► emits events ──► Event Store (UmaDB)
                                  │
                ┌─────────────────┴─────────────────┐
                ▼                                   ▼
           Projector                              Effect
           (builds read model                    (side effects:
            in SQLite)                           HTTP, execute commands)
                                                     │
                                                     ▼
                                                  Command
                                                  (private, for
                                                   idempotency)
```

## Prerequisites

- **UmaDB**: the event store. Must be running before starting the Umari server
- **A toolchain for your SDK** of choice:

{{#tabs global="lang" }}
{{#tab name="Rust" }}

- **Rust**: modules are written with the `umari` SDK crate
- **wasm32-wasip2 target**: `rustup target add wasm32-wasip2`

{{#endtab }}
{{#tab name="TypeScript" }}

- **Node.js**: modules are written with the `@umari/js` SDK
- **`@bytecodealliance/jco` + `esbuild`**: used by `umari-js build` to bundle and componentize your TypeScript to WASM (installed as dev dependencies)

{{#endtab }}
{{#endtabs }}

## What you'll build

A typical Umari application looks like this:

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```
my-project/
├── src/                     # Shared library crate: events, folds
│   ├── events/
│   │   ├── user.rs
│   │   ├── project.rs
│   │   └── task.rs
│   └── folds/
│       └── mod.rs
├── commands/
│   ├── register-user/       # Crate: register-user
│   ├── create-project/
│   └── cancel-project/
├── projectors/
│   ├── projects/            # Crate: projects
│   ├── users/
│   └── tasks/
├── effects/
│   ├── register-webhooks/
│   └── create-project/
└── Cargo.toml               # Workspace root
```

Each command, projector, and effect is its own crate, compiled as a WASM component. They all depend on the shared library crate for event and fold definitions.

{{#endtab }}
{{#tab name="TypeScript" }}

```
my-project/
├── shared/                  # Shared workspace package: @my-project/shared
│   └── src/
│       ├── events/
│       │   ├── user.ts
│       │   ├── project.ts
│       │   └── task.ts
│       └── index.ts
├── commands/
│   ├── register-user/       # Package: register-user
│   ├── create-project/
│   └── cancel-project/
├── projectors/
│   ├── projects/            # Package: projects
│   ├── users/
│   └── tasks/
├── effects/
│   ├── register-webhooks/
│   └── create-project/
└── package.json             # npm workspace root
```

Each command, projector, and effect is its own npm workspace package, compiled as a WASM component. They all depend on the shared package for event and fold definitions.

{{#endtab }}
{{#endtabs }}

> Both layouts are generated for you by `umari init`; see [Project Structure](./project-structure.md).

## About this book

This book is the canonical reference for building on Umari. It walks through the runtime, the SDK, and the patterns you'll use to write commands, projectors, and effects.
