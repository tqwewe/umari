---
name: umari
description: Write event-sourced WASM modules with Umari — commands, projectors, effects, folds, domain IDs, and the `umari` CLI for scaffolding, building, and deploying. Modules can be written in Rust (the `umari` SDK crate) or TypeScript (the `@umari/js` SDK); both compile to the same WASM contract. Use when the user wants to write or modify Umari modules, design events/folds/domain IDs, build the fold-check → side effect → record idempotency pattern, scaffold via `umari init` / `umari new`, or use `Command::new`, `#[export_command]`, `export_projector!`, `export_effect!`, `Projector`, `Effect`, `Fold`, `EventSet`, `DomainIds` (Rust) or `defineCommand`, `defineProjector`, `defineEffect`, `defineEvent`, `defineFold`, `exportCommand`, `umari-js build` (TypeScript). Also use when configuring `wasm32-wasip2` builds, npm workspaces, `[package.metadata.umari.env]`, `Cargo.toml`, or `package.json` for a Umari workspace.
---

# Umari skill

Umari is a WASM-native event-sourcing runtime. Business logic compiles to WebAssembly components and runs inside a Wasmtime-based runtime. Three module types: **commands** (write events), **projectors** (build SQLite read models), **effects** (call external systems). Consistency uses **Dynamic Consistency Boundaries (DCB)** — no aggregates, no per-entity streams. Each command/fold declares which events it cares about via an event set + domain IDs, and the runtime forms the consistency boundary on the fly.

**Two SDKs, one runtime.** Modules can be written with the Rust SDK (`umari`, `crates/umari` in this repo) or the TypeScript SDK (`@umari/js`, `packages/js`). Both target the same WIT contract and produce interchangeable WASM components — a project can mix languages. Rust builds for `wasm32-wasip2` (`cdylib` + `rlib`); TypeScript bundles with esbuild and componentizes via jco (`umari-js build`). The CLI is `umari` for both.

**Pick the language from the workspace.** A Cargo workspace (`Cargo.toml` with `[workspace]`) → Rust. An npm workspace (`package.json` with `"workspaces"`) → TypeScript. `umari new` infers this automatically. If starting fresh, `umari init` scaffolds either (prompts for language, or `--lang rust|js`).

## Mental model — the three module types

| Module | Reads | Writes | Rust | TypeScript | When |
|---|---|---|---|---|---|
| **Command** | Events (via folds) | Events | `#[export_command]` on `pub fn execute(input, context)` | `defineCommand({…})` + `exportCommand` | The only writers. Validate input → replay relevant events through folds → emit new events. |
| **Projector** | Events | Own SQLite DB | `impl Projector` + `export_projector!(T)` | `defineProjector({…})` + `exportProjector` | Build read models. Deterministic & idempotent — replay deletes the DB and reprocesses from position 0. |
| **Effect** | Events | External world | `impl Effect` + `export_effect!(T)` | `defineEffect({…})` + `exportEffect` | Call HTTP, send emails, call other commands. Re-runnable via the fold-check → side effect → record pattern. |

**Commands are the only writers.** Projectors and effects subscribe but never emit. Effects call commands (which write events) using the private-command pattern.

## The canonical command shape

**Rust:**

```rust
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
            }])
        })
}
```

Folds register IN ORDER; the execute closure receives their states as a tuple in that same order. Max 12 folds per command.

**TypeScript:**

```ts
import { defineCommand, exportCommand } from "@umari/js";
import { UserExistsFold, ProjectFold, ProjectCreated } from "../shared/index.js";
import { z } from "zod";

const InputSchema = z.object({
  userId: z.bigint(),
  projectId: z.string(),
  title: z.string().min(1).max(200),
});
type Input = z.infer<typeof InputSchema>;

const CreateProject = defineCommand<Input, {
  userExists: ReturnType<typeof UserExistsFold>;
  project: ReturnType<typeof ProjectFold>;
}>({
  input: InputSchema, // optional zod schema — validates before execute
  domainIds: ["userId", "projectId"] as const,
  folds: ({ userId, projectId }) => ({
    userExists: UserExistsFold({ userId }),
    project: ProjectFold({ projectId }),
  }),
  execute: ({ input, folds, emit, reject }) => {
    if (!folds.userExists) reject("user does not exist");
    if (folds.project.exists) reject("project already exists");
    return emit(ProjectCreated({
      projectId: input.projectId,
      userId: input.userId,
      title: input.title,
    }));
  },
});

export const { schema, execute } = exportCommand(CreateProject);
```

Folds are a NAMED MAP; the same keys appear on `folds` in `execute`. Call `reject(msg)` for a business-rule violation, `invalidInput(msg)` for bad input. `emit(...)` takes 0+ event payloads.

## Rust ↔ TypeScript mapping

| Concept | Rust | TypeScript |
|---|---|---|
| Define event | `#[derive(Event, DomainIds, …)]` + `#[event_type]` | `defineEvent<Data>()(type, { domainIds })` |
| Event set | `#[derive(EventSet)] enum Query` | `events: [A, B]` array |
| Domain IDs | `#[domain_id]` fields (snake_case) | `domainIds: [...]` (camelCase) |
| Define fold | `impl Fold` (state mutated via `&mut`) | `defineFold({ domainIds, events, initial, apply })` (apply returns next state, reduce-style; may mutate object state) |
| Bind a fold | `.fold::<T>()` (via `FromDomainIds`) | `T({ …bindings })` in the `folds` map |
| Emit / reject | `emit![…]` / `anyhow::ensure!`,`bail!` | `emit(…)` / `reject(msg)`, `invalidInput(msg)` |
| Crypto scope | `#[crypto_scope]` on a `#[domain_id]` field | `cryptoScope: (data) => "prefix:value"` |
| Call a command | private fn call | `execute(name, input, ctx?)` (returns `void`) |
| Standalone folds | `FoldQuery::new()…run()` | `foldQuery({ … }).run()` |
| SQLite | free fns + `Statement`, `params!`, `?1` | `sqlite.*` namespace, params array, `?` |
| HTTP (effects) | `wasi-http-client` | global `fetch` |
| Env vars | `env::var(...)` | `env(name)` / `envOptional(name)` |
| Build target | `wasm32-wasip2`, `cdylib`+`rlib` | esbuild bundle → jco componentize |

## Critical invariants (do not violate)

**Both languages:**
1. **Commands are the only writers.** Projectors and effects subscribe but never emit; effects record outcomes by calling a (usually private) command.
2. **Effects must use the fold-check → side effect → record pattern.** Anchor idempotency in the event store (a completion event checked via a fold), never in SQLite, timestamps, or external state.
3. **Projector `init` runs before every replay** — always `CREATE TABLE IF NOT EXISTS`. `handle` runs in an implicit transaction; never `BEGIN`/`COMMIT`.
4. **SQLite surfaces only constraint violations** — wrong types, unknown columns, "expected one row" all TRAP the module.
5. **A tag is `<field>:<value>`** — the field name IS part of the contract. Rust uses snake_case (`user_id`), TypeScript camelCase (`userId`); keep them consistent if mixing languages over the same events.

**Rust-specific:**
6. `#[derive(Event)]` does NOT imply `DomainIds` — events need both. Fold structs need `#[derive(DomainIds, FromDomainIds)]`.
7. `#[crypto_scope]` must be on a field that is ALSO `#[domain_id]`. EventSet enum is conventionally `Query`. `#[scope(field)]` narrows a variant's filter; without it ALL the fold's bindings filter every event type.
8. Module crates declare `crate-type = ["cdylib", "rlib"]` and build for `wasm32-wasip2`.

**TypeScript-specific:**
9. **Fold `apply` is reduce-style (like `Array.reduce`): return the next state.** A primitive fold returns the new value (`initial: () => false; apply: () => true`). For object/array state you may mutate in place instead — returning nothing keeps the mutated state.
10. Imports use the `.js` extension (`from "../shared/index.js"`) even for `.ts` source. Domain IDs and the `position` field arrive as `bigint`; commands validate input with an optional `zod` schema on `input`.

## How to use this skill

Read the reference files on demand — they contain the full patterns, exact derives/signatures, common pitfalls, and verbatim templates. **Do not paraphrase from memory; the references are the source of truth.**

| If the user wants to… | Read |
|---|---|
| Write modules in **TypeScript / JS** | `reference/javascript.md` (the JS counterpart to every Rust reference below) |
| Write or modify a **command** | `reference/commands.md` (and `reference/folds.md`) — Rust; `reference/javascript.md` — TS |
| Write or modify a **projector** | `reference/projectors.md` — Rust; `reference/javascript.md` — TS |
| Write or modify an **effect** | `reference/effects.md` (and `reference/idempotency.md`) — Rust; `reference/javascript.md` — TS |
| Design **events** or **domain IDs** | `reference/events.md`, `reference/domain-ids.md` |
| Build a **fold** (custom or built-in) | `reference/folds.md` |
| Encrypt events or **crypto-shred** | `reference/crypto.md` |
| Set up a new **workspace** or module | `reference/project-structure.md`, `reference/cli.md` (covers `umari init` for both languages) |
| Use the **`umari` CLI** (init/new/build/deploy/replay) | `reference/cli.md` |
| Make a command **idempotent** | `reference/idempotency.md` |
| Avoid common mistakes | `reference/pitfalls.md` |

> The per-topic references are Rust-first with a **TypeScript** callout at the top of each (the `@umari/js` equivalent + a link into `reference/javascript.md`, which is the full JS reference). The concepts (DCB, domain IDs, folds, the idempotency pattern) are language-neutral. The canonical generic example domain is **user / project / task**.

Verbatim Rust file templates (matching `umari new` output) live in `templates/`. For TypeScript, the shapes in `reference/javascript.md` and `packages/js/examples` are the templates.

## Workflow when the user asks for a new module

1. **Identify the module type** — command, projector, or effect? If unclear, ask once.
2. **Determine the language** — Cargo workspace → Rust; npm workspace (`package.json` with `"workspaces"`) → TypeScript. If no workspace exists, scaffold with `umari init` (read `reference/cli.md`).
3. **Read the relevant reference** before writing code — `reference/javascript.md` for TypeScript, the per-topic file for Rust. The builder/define patterns, fold composition, and emit semantics are easy to get subtly wrong.
4. **Prefer `umari new <type> <name>`** in an existing workspace — it scaffolds the module, wires it in (Cargo `members` / npm workspace + `@<project>/shared` dep), and infers the language.
5. **Verify the build** — Rust modules are `cdylib`s built with `umari build` (`cargo build --target wasm32-wasip2`); TypeScript modules build via `umari build` / `umari-js build`. Don't suggest `cargo run` / `node` on module code.

## What this skill does NOT cover

- Authoring the runtime, server, or CLI internals (`crates/api`, `crates/runtime`, `crates/server`, `crates/cli`). Use general Rust knowledge for those.
- UmaDB internals — the event store is opaque. Just know it must be running at `UMARI_EVENT_STORE_URL` (default `http://localhost:50051`).
