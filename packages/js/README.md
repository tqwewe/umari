# `@umari/js`

TypeScript SDK for writing [Umari](https://umari.tqwewe.com) modules. Build
event-sourced commands, projectors, and effects in TS, compile them to WASM
components via `jco componentize`, and run them on the Umari runtime.

> Note: this is the JS/TS counterpart to the Rust SDK under
> `crates/umari/`. The two SDKs share the same WIT contract and produce
> interchangeable `.wasm` modules.

## Install

```sh
npm install --save-dev @umari/js @bytecodealliance/jco esbuild
```

## Write a command

```ts
// commands/register-user/src/index.ts
import {
  defineEvent,
  defineFold,
  defineCommand,
  exportCommand,
  EventFold,
} from "@umari/js";

type UserRegisteredData = {
  userId: bigint;
  email: string;
  name: string;
};

export const UserRegistered = defineEvent<UserRegisteredData>()("user.registered", {
  domainIds: ["userId"],
});

type Input = {
  userId: bigint;
  email: string;
  name: string;
};

const RegisterUser = defineCommand<Input, Record<string, never>>({
  domainIds: ["userId"] as const,
  folds: ({ userId }) => ({
    connected: EventFold(UserRegistered)({ userId }),
  }),
  execute: ({ input, folds, emit }) => {
    if (folds.connected.length > 0) return emit(); // idempotent no-op
    return emit(
      UserRegistered({
        userId: input.userId,
        email: input.email,
        name: input.name,
      }),
    );
  },
});

export const { schema, execute } = exportCommand(RegisterUser);
```

## Write a projector

```ts
// projectors/users/src/index.ts
import { defineProjector, exportProjector, sqlite } from "@umari/js";
import { UserRegistered } from "../events.js";

const UsersProjector = defineProjector({
  events: [UserRegistered],
  init: () => {
    sqlite.executeBatch(`
      CREATE TABLE IF NOT EXISTS users (
        user_id TEXT PRIMARY KEY,
        email TEXT NOT NULL,
        name TEXT NOT NULL
      );
    `);
  },
  handle: (event) => {
    switch (event.type) {
      case "user.registered":
        sqlite.execute(
          "INSERT INTO users (user_id, email, name) VALUES (?, ?, ?)",
          [event.data.userId, event.data.email, event.data.name],
        );
        break;
    }
  },
});

export const { projector } = exportProjector(UsersProjector);
```

## Write an effect

```ts
// effects/notify-owner/src/index.ts
import { defineEffect, exportEffect, env } from "@umari/js";
import { UserRegistered } from "../events.js";

const NotifyOwner = defineEffect({
  events: [UserRegistered],
  init: () => ({ endpoint: env("NOTIFY_ENDPOINT") }),
  partitionKey: (event) => event.data.userId.toString(),
  handle: async (event, state) => {
    const r = await fetch(state.endpoint, {
      method: "POST",
      body: JSON.stringify({ userId: event.data.userId.toString() }),
    });
    if (!r.ok) throw new Error(`status ${r.status}`);
  },
});

export const { effect } = exportEffect(NotifyOwner);
```

## Build

```sh
npx umari-js build src/index.ts --out dist/module.wasm
```

The CLI detects the module kind from the entry file's exports
(`exportCommand` / `exportProjector` / `exportEffect`) and targets the right
WIT world automatically.

## Concepts

- **Events** carry a payload + a list of *domain id* fields. Domain ids tag
  the event in the store so it can be efficiently queried.
- **Folds** reduce a stream of events keyed by domain ids into in-memory
  state. Commands declare which folds they need; the runtime fetches and
  replays only matching events before invoking your `execute`.
- **Commands** are the only writers. They take a typed input, replay folds,
  validate invariants, and emit new events.
- **Projectors** consume events and build SQLite read models.
- **Effects** consume events and perform side effects (HTTP, calling other
  commands). Use `partitionKey` to serialise events that share a domain id.

See [the Umari book](https://umari.tqwewe.com) for the full conceptual
overview.

## On `bigint`

Umari uses `bigint` everywhere a Rust `i64` / `u64` would appear: event store
positions, timestamps in the WIT layer, and any payload field declared as
`bigint`. Payload JSON serialises `bigint` as a decimal string by
convention. Inside `handle` / `execute`, coerce explicitly with `BigInt(...)`
when reading payload fields that round-trip through the wire.
