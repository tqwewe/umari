# 1. Introduction

Umari is a **WASM-native event sourcing runtime**. You write business logic in Rust, compile it to WebAssembly, and the runtime handles event persistence, module lifecycle, and state derivation.

## What is event sourcing?

In event sourcing, every state change is recorded as an **immutable event** in an append-only log. The current state of the system is derived by replaying events from the beginning. This gives you:

- **Full audit trail** — every change is recorded, forever
- **Temporal queries** — ask "what was the state at time T?" by replaying up to that point
- **Complete replayability** — delete all derived state and rebuild it from events
- **Causal traceability** — every event knows which action produced it and which event triggered that action

Umari adds two key innovations on top of classical event sourcing:

1. **No aggregates, no streams** — consistency boundaries are dynamic (DCB), not pre-partitioned
2. **Everything is WASM** — business logic is compiled to WebAssembly and loaded at runtime

## Three module types

Umari splits business logic into three distinct concerns, each compiled as a separate WASM module:

| Module | Role | Writes events? | Has SQLite? | Idempotency |
|--------|------|---------------|-------------|-------------|
| **Command** | Validate input, enforce invariants, emit events | Yes (only writer) | No | Fold state check in execute closure |
| **Projector** | Build read models from events | No | Yes (external reads OK) | Structural: same events → same SQLite |
| **Effect** | React to events, perform side effects (HTTP, send email) | Via commands only | Yes (internal only) | Fold-check → side effect → record |

Commands are the **only mechanism for writing events**. Projectors and effects subscribe to events and react. Effects can trigger commands, which write events, which trigger more effects — forming a causal chain.

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

- **Rust** — you write modules in Rust using the `umari` SDK crate
- **UmaDB** — the event store. Must be running before starting the Umari server
- **wasm32-wasip2 target** — `rustup target add wasm32-wasip2`

## What you'll build

A typical Umari application looks like this:

```
my-project/
├── src/                     # Shared library: events, folds
│   ├── events/
│   │   ├── shop.rs
│   │   ├── warranty.rs
│   │   └── claim.rs
│   └── folds/
│       └── mod.rs
├── commands/
│   ├── connect-shop/        # Crate: connect-shop
│   ├── create-warranty-plan/
│   └── cancel-warranty/
├── projectors/
│   ├── plans/               # Crate: plans
│   ├── shops/
│   └── warranties/
├── effects/
│   ├── register-shopify-webhooks/
│   └── record-warranty-sale/
└── Cargo.toml               # Workspace root
```

Each command, projector, and effect is its own crate, compiled as a WASM component. They all depend on the shared library for event and fold definitions.

## About this book

This book is the canonical reference for building on Umari. It walks through the runtime, the SDK, and the patterns you'll use to write commands, projectors, and effects.
