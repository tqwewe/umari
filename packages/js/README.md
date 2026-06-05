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
// commands/connect-shop/src/index.ts
import {
  defineEvent,
  defineFold,
  defineCommand,
  exportCommand,
  EventFold,
} from "@umari/js";

type ShopConnectedData = {
  shopId: bigint;
  shopDomain: string;
  accessToken: string;
};

export const ShopConnected = defineEvent<ShopConnectedData>()("shop.connected", {
  domainIds: ["shopId"],
});

type Input = {
  shopId: bigint;
  shopDomain: string;
  accessToken: string;
};

const ConnectShop = defineCommand<Input, Record<string, never>>({
  domainIds: ["shopId"] as const,
  folds: ({ shopId }) => ({
    connected: EventFold(ShopConnected)({ shopId }),
  }),
  execute: ({ input, folds, emit }) => {
    if (folds.connected.length > 0) return emit(); // idempotent no-op
    return emit(
      ShopConnected({
        shopId: input.shopId,
        shopDomain: input.shopDomain,
        accessToken: input.accessToken,
      }),
    );
  },
});

export const { schema, execute } = exportCommand(ConnectShop);
```

## Write a projector

```ts
// projectors/shops/src/index.ts
import { defineProjector, exportProjector, sqlite } from "@umari/js";
import { ShopConnected } from "../events.js";

const ShopsProjector = defineProjector({
  events: [ShopConnected],
  init: () => {
    sqlite.executeBatch(`
      CREATE TABLE IF NOT EXISTS shops (
        shop_id TEXT PRIMARY KEY,
        shop_domain TEXT NOT NULL,
        access_token TEXT NOT NULL
      );
    `);
  },
  handle: (event) => {
    switch (event.type) {
      case "shop.connected":
        sqlite.execute(
          "INSERT INTO shops (shop_id, shop_domain, access_token) VALUES (?, ?, ?)",
          [event.data.shopId, event.data.shopDomain, event.data.accessToken],
        );
        break;
    }
  },
});

export const { projector } = exportProjector(ShopsProjector);
```

## Write an effect

```ts
// effects/notify-owner/src/index.ts
import { defineEffect, exportEffect, env } from "@umari/js";
import { ShopConnected } from "../events.js";

const NotifyOwner = defineEffect({
  events: [ShopConnected],
  init: () => ({ endpoint: env("NOTIFY_ENDPOINT") }),
  partitionKey: (event) => event.data.shopId.toString(),
  handle: async (event, state) => {
    const r = await fetch(state.endpoint, {
      method: "POST",
      body: JSON.stringify({ shopId: event.data.shopId.toString() }),
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
