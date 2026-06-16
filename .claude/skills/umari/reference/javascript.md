# TypeScript / JavaScript SDK (`@umari/js`)

The TypeScript SDK is the counterpart to the Rust `umari` crate. It targets the same WIT contract and produces interchangeable WASM components. This file gives the `@umari/js` syntax for every concept; the per-topic Rust references (`events.md`, `folds.md`, `commands.md`, `projectors.md`, `effects.md`, `domain-ids.md`, `idempotency.md`) explain the underlying model, which is identical across languages.

Generic example domain used throughout: **user / project / task**.

## Setup & workspace

A TypeScript project is an **npm workspaces** project with a dedicated `shared` package:

```
my-project/
├── package.json              # npm workspace root + dev tooling
├── tsconfig.json
├── shared/                   # @my-project/shared — events + folds
│   ├── package.json
│   └── src/index.ts
├── commands/<name>/          # one package per command
├── projectors/<name>/
└── effects/<name>/
```

Scaffold with `umari init --lang js` (workspace) and `umari new command|projector|effect <name>` (modules — infers the language from the npm workspace, wires in the `@<project>/shared` dependency; run `npm install` from the root afterward).

Root `package.json` workspaces + dev tooling (hoisted to every package):

```json
{
  "workspaces": ["shared", "commands/*", "projectors/*", "effects/*"],
  "devDependencies": {
    "@umari/js": "^0.1.0",
    "@bytecodealliance/jco": "^1.24.1",
    "@types/node": "^25.9.3",
    "esbuild": "^0.28.1",
    "typescript": "^6.0.3"
  }
}
```

Each module `package.json` declares the wasm output and the build script:

```json
{
  "name": "create-project",
  "type": "module",
  "umari": { "wasm": "dist/module.wasm" },
  "scripts": { "build": "umari-js build src/index.ts --out dist/module.wasm" },
  "devDependencies": { "@umari/js": "^0.1.0", "@my-project/shared": "*" }
}
```

> Module capability model: commands and projectors are deterministic and network-free; only **effects** get `wasi:http` (`fetch`). `umari-js build` strips http from command/projector builds.

**Import convention:** module specifiers use the `.js` extension even though the source is `.ts` (`import { UserRegistered } from "../shared/index.js";`). ESM, `"type": "module"`.

## Events

```ts
import { defineEvent } from "@umari/js";

export type UserRegisteredData = {
  userId: bigint;   // numeric domain ids are bigint
  email: string;
  name: string;
};

export const UserRegistered = defineEvent<UserRegisteredData>()("user.registered", {
  domainIds: ["userId"],                       // tag = field name, e.g. userId:42
  cryptoScope: (data) => `user_id:${data.userId}`, // optional encryption scope
});
```

- `defineEvent<Data>()(type, options)` — curried so you annotate `Data` while `type`/`domainIds` are inferred.
- The result is a callable factory: `UserRegistered({ userId, email, name })` builds an emit payload. It also carries `.type` and `.domainIds`.
- **Tag names are the field name as written** (camelCase). To stay interoperable with Rust modules over the same events, keep field names aligned (Rust snake_case `user_id` vs TS `userId` produce DIFFERENT tags).
- `cryptoScope` returning a string encrypts the whole payload under that scope's key; returning `undefined` (or omitting) stores plaintext.

## Domain IDs

Declared as a `domainIds` array on events, commands, and folds — the keys whose values become tags / bindings. There is no `DomainIds`/`FromDomainIds` trait; folds are bound by passing the fields explicitly (see Folds). Numeric ids are `bigint`, UUIDs are `string`.

## Folds

```ts
import { defineFold } from "@umari/js";
import { UserRegistered, UserReactivated } from "../events/user.js";

export const UserEmailFold = defineFold({
  domainIds: ["userId"] as const,   // scope every event by these bindings
  events: [UserRegistered, UserReactivated],
  initial: () => ({ email: undefined as string | undefined }),
  apply: (state, event) => {
    switch (event.type) {           // discriminates event.data
      case "user.registered":
      case "user.reactivated":
        state.email = event.data.email;
        break;
    }
  },
});
```

**`apply` is reduce-style: return the next state** (like `Array.reduce`). For a primitive state, return the new value. For an object/array state you may instead mutate it in place — returning nothing keeps the mutated state (the runtime overwrites the state only when `apply` returns a value).

```ts
// primitive state — return the next value
defineFold({ /*…*/ initial: () => false, apply: () => true });
// object state — mutate in place (return optional)
defineFold({ /*…*/ initial: () => ({ count: 0n }), apply: (s) => { s.count += 1n; } });
```

The example above mutates `state.email` (object accumulator). The runner keeps the mutated `state` because `apply` returns nothing.

Built-in folds (mirror Rust `EventFold`/`LatestEvent`/`EventCounter`/`EventToggle`) — call the helper, then bind:

| Built-in | Bind | State |
|---|---|---|
| `EventFold(E)` | `EventFold(E)({ userId })` | `StoredEvent<E>[]` (use `.length`) |
| `LatestEvent(E)` | `LatestEvent(E)({ projectId })` | `{ value?: StoredEvent<E> }` |
| `EventCounter(E)` | `EventCounter(E)({ projectId })` | `{ count: bigint }` |
| `EventToggle(A, B)` | `EventToggle(A, B)({ projectId })` | `{ last?: { side: "a"\|"b"; event } }` |

The fold's `domainIds` array is the equivalent of Rust's `#[scope(field)]` — it narrows which bindings filter each event. There is no per-event hardcoded-literal scope in the TS SDK.

## Commands

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
  input: InputSchema,                 // optional — validates before execute
  domainIds: ["userId", "projectId"] as const,
  folds: ({ userId, projectId }) => ({
    userExists: UserExistsFold({ userId }),
    project: ProjectFold({ projectId }),
  }),
  execute: ({ input, folds, context, emit, reject, invalidInput }) => {
    if (!folds.userExists) reject("user does not exist");             // business rule
    if (folds.project.exists) reject("project already exists");
    return emit(ProjectCreated({
      projectId: input.projectId,
      userId: input.userId,
      title: input.title,
    }));                              // emit() for a no-op
  },
});

export const { schema, execute } = exportCommand(CreateProject);
```

- `folds` returns a **named map**; the same keys appear on `folds` inside `execute`.
- `execute` args: `{ input, folds, context, emit, reject, invalidInput }`.
- `reject(msg)` → business-rule failure; `invalidInput(msg)` → bad input; either throws and short-circuits.
- `emit(...)` collects 0+ event payloads. Return it.
- `input` is an optional schema (any object exposing `parse`/`jsonSchema`, e.g. zod). Without it, raw input is passed through untyped.
- Idempotency: the runtime checks `context.idempotencyKey` against fold-scope events; on a match the command commits empty without running `execute`. Domain-level idempotency: return `emit()` when the desired state already holds.

## Projectors

```ts
import { defineProjector, exportProjector, sqlite } from "@umari/js";
import { ProjectCreated, TaskCreated } from "../shared/index.js";

const Projects = defineProjector({
  events: [ProjectCreated, TaskCreated],
  init: () => {
    sqlite.executeBatch(`
      CREATE TABLE IF NOT EXISTS projects (
        project_id TEXT PRIMARY KEY,
        title TEXT,
        task_count INTEGER NOT NULL DEFAULT 0
      );
    `);
    // Optionally return state (e.g. prepared statements) passed to handle as 2nd arg.
  },
  handle: (event) => {
    switch (event.type) {
      case "project.created":
        sqlite.execute("INSERT INTO projects (project_id, title) VALUES (?, ?)",
          [event.data.projectId, event.data.title]);
        break;
      case "task.created":
        sqlite.execute("UPDATE projects SET task_count = task_count + 1 WHERE project_id = ?",
          [event.data.projectId]);
        break;
    }
  },
});

export const { projector } = exportProjector(Projects);
```

- Naturally idempotent: replay deletes the DB and reprocesses from position 0. Always `CREATE TABLE IF NOT EXISTS`.
- Whatever `init` returns is passed back to `handle` as the second argument — keep prepared statements there.

## Effects

```ts
import { defineEffect, exportEffect, env, execute, foldQuery, EventFold } from "@umari/js";
import { UserRegistered, WebhookRegistered } from "../shared/index.js";

const RegisterWebhooks = defineEffect({
  events: [UserRegistered],
  init: () => ({ webhookAddress: env("WEBHOOK_ADDRESS") }), // throws if missing; envOptional() otherwise
  partitionKey: (event) => event.data.userId.toString(),    // undefined = sequential
  handle: async (event, state) => {                         // async: await fetch etc.
    const { userId } = event.data;

    // 1. FOLD-CHECK — already done?
    const { registered } = foldQuery({
      registered: EventFold(WebhookRegistered)({ userId }),
    }).run();
    if (registered.length > 0) return;

    // 2. SIDE EFFECT
    const res = await fetch(`${state.webhookAddress}/webhooks`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ userId: userId.toString() }),
    });
    if (!res.ok) { console.error(`failed: ${res.status}`); return; } // throw/return → retried with backoff

    // 3. RECORD — via a (usually private) command
    execute("record-webhook", { userId }, {
      correlationId: event.correlationId,
      triggeringEventId: event.id,
      idempotencyKey: event.id,
    });
  },
});

export const { effect } = exportEffect(RegisterWebhooks);
```

- **Idempotency: fold-check → side effect → record.** Anchor in the event store (a completion event), never SQLite/timestamps/external state.
- `handle` is **async** — `await` fetch/promises. Throwing (or a rejected promise) marks the event failed → retried indefinitely with exponential backoff (1s → 10m cap), position never advances past the failure.
- `execute(name, input, ctx?)` invokes another command by name and **returns `void`** (no receipt). To decide whether to act, check state with `foldQuery({...}).run()` first. Omitted context fields derive from the current event; pass `idempotencyKey: event.id` for dedup.
- HTTP is `fetch` (effects only). Env via `env(name)` / `envOptional(name)`.
- Effects can define their own private events + commands locally (a `record-webhook.ts` command deployed alongside).

## SQLite (`sqlite` namespace)

`import { sqlite } from "@umari/js"` (or `import * as sqlite from "@umari/js/sqlite"`). Positional `?` placeholders with a params array.

```ts
sqlite.execute(sql, params?)      // -> bigint (rows affected); throws SqliteError on constraint violation
sqlite.executeBatch(sql)          // -> void (DDL in init)
sqlite.queryOne(sql, params?)     // -> Row (throws on 0 or >1 rows)
sqlite.queryRow(sql, params?)     // -> Row | undefined
sqlite.query(sql, params?)        // -> Row[]
sqlite.lastInsertRowid()          // -> bigint | undefined
sqlite.prepare(sql)               // -> PreparedStatement { execute, query, queryOne, queryRow }
```

Reading rows — `row.get(col, as?)` coerces; without `as` returns the natural value:

```ts
row.get("name", "string");
row.get(0, "bigint");   // "bigint"|"number"|"string"|"boolean"|"uint8array"|"date"
```

Param types: `null | bigint | number | boolean | string | Uint8Array`. Store UUIDs as `TEXT`. Only constraint violations are recoverable (they throw a `SqliteError` with `tag === "constraint-violation"`); everything else traps. `handle` runs in an implicit transaction — no `BEGIN`/`COMMIT`.

## Build & deploy

```sh
umari build [paths...]            # bundle (esbuild) → componentize (jco) → wasm, for every module
umari deploy [paths...]           # build + upload + activate
umari-js build src/index.ts --out dist/module.wasm   # build one module directly
```

TypeScript modules currently receive env vars at upload time via the CLI/API (`--env KEY=VALUE`); the package.json `umari` field only carries `wasm`. See `reference/cli.md`.

## Public API (selected exports)

`defineEvent`, `defineFold`, `defineCommand`, `defineProjector`, `defineEffect`; `exportCommand`, `exportProjector`, `exportEffect`; `emit`, `reject`, `invalidInput`, `execute`, `foldQuery`; `EventFold`, `LatestEvent`, `EventCounter`, `EventToggle`; `sqlite`, `env`, `envOptional`, `deleteCryptoKey`; types `StoredEvent`, `CommandContext`, `EventDef`, `BoundFold`.
